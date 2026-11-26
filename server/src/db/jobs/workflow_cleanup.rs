const WORKFLOW_CLEANUP_PROTOCOL_VERSION: i64 = 3;
const WORKFLOW_LEGACY_TYPE: &str = "applicationWorkflow";
const WORKFLOW_V2_TYPE: &str = "applicationWorkflowV2";
const WORKFLOW_LEGACY_PAGE_SIZE: i64 = 100;
const WORKFLOW_LEGACY_MAX_PAGE_INDEX: i64 = 4_095;
const WORKFLOW_LEGACY_TOKEN_MAX_BYTES: usize = 4_096;
const WORKFLOW_LEGACY_REVALIDATION_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const WORKFLOW_CLEANUP_RETRY_MS: i64 = 5_000;
const WORKFLOW_CLEANUP_QUERY_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-query-v3\0";
const WORKFLOW_CLEANUP_GENESIS_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-genesis-v3\0";
const WORKFLOW_CLEANUP_PAGE_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-page-v3\0";
const WORKFLOW_CLEANUP_TARGETS_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-targets-v3\0";
const WORKFLOW_CLEANUP_TARGET_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-target-v3\0";
const WORKFLOW_CLEANUP_EVIDENCE_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-workflow-cleanup-evidence-v3\0";
const WORKFLOW_CLEANUP_ZERO_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-zero-v3\0";
const WORKFLOW_CLEANUP_COMPLETION_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-completion-v3\0";
const WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN: &[u8] = b"bluey-jobs-v2-known-run-set-v3\0";
const WORKFLOW_CLEANUP_V2_TARGET_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-workflow-v2-target-v3\0";
const WORKFLOW_CLEANUP_SWEEP_SCOPE_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-sweep-scope-v3\0";
const WORKFLOW_CLEANUP_SWEEP_MANIFEST_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-sweep-manifest-v3\0";
const WORKFLOW_CLEANUP_SWEEP_SCOPE_SET_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-sweep-scope-set-v3\0";
const WORKFLOW_CLEANUP_SWEEP_AUTHORIZATION_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-sweep-authorization-v3\0";
const WORKFLOW_CLEANUP_SWEEP_RUNNER_AUTHORITY_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-sweep-runner-authority-v3\0";
const WORKFLOW_CLEANUP_HARD_DELETE_AUTHORIZATION_DIGEST_DOMAIN: &[u8] =
    b"bluey-jobs-workflow-cleanup-hard-delete-authorization-v3\0";
const WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS: usize = 32;
const WORKFLOW_CLEANUP_MAX_SWEEP_SCOPES: usize = 2;
const WORKFLOW_CLEANUP_MAX_MANAGED_CLOUD_MEMO_BYTES: usize = 4_096;

fn sqlite_workflow_cleanup_db_now_ms(tx: &rusqlite::Transaction<'_>) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
        [],
        |row| row.get(0),
    )?)
}

fn postgres_workflow_cleanup_db_now_ms(tx: &mut postgres::Transaction<'_>) -> Result<i64> {
    Ok(tx
        .query_one(
            "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
            &[],
        )?
        .get(0))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsLegacyInventoryPageLease {
    pub cleanup_request_id: String,
    pub inventory_generation_id: String,
    pub namespace: String,
    pub workflow_type: String,
    pub visibility_cutoff_ms: i64,
    pub query_digest: String,
    pub scan_pass: i64,
    pub page_index: i64,
    pub predecessor_page_digest: Option<String>,
    pub page_token: Option<String>,
    pub cleanup_fence: i64,
    pub request_epoch: i64,
    pub lease_owner: String,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsLegacyTargetCleanupLease {
    pub cleanup_request_id: String,
    pub inventory_generation_id: String,
    pub namespace: String,
    pub workflow_type: String,
    pub visibility_cutoff_ms: i64,
    pub query_digest: String,
    pub scan_pass: i64,
    pub workflow_id: String,
    pub run_id: String,
    pub first_execution_run_id: String,
    pub target_digest: String,
    pub cleanup_fence: i64,
    pub observation_pass: i64,
    pub proof_epoch: i64,
    pub request_epoch: i64,
    pub lease_owner: String,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsV2TargetCleanupLease {
    pub cleanup_request_id: String,
    pub cleanup_generation_id: String,
    pub target_set_digest: String,
    pub namespace: String,
    pub workflow_type: String,
    pub workflow_id: String,
    pub first_execution_run_id: Option<String>,
    pub start_request_id: String,
    pub start_payload_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_cloud_binding_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_cloud_release_memo_base64url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_cloud_release_memo_sha256: Option<String>,
    pub known_run_ids: Vec<String>,
    pub target_digest: String,
    pub cleanup_fence: i64,
    pub observation_pass: i64,
    pub request_epoch: i64,
    pub lease_owner: String,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum JobsWorkflowCleanupWorkLease {
    LegacyInventoryPage(JobsLegacyInventoryPageLease),
    ReconcileLegacyTarget(JobsLegacyTargetCleanupLease),
    ReconcileV2Target(JobsV2TargetCleanupLease),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobsWorkflowCleanupDeliveryFailure {
    TransportUnknown,
    GatewayUnavailable,
    IdentityConflict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobsWorkflowCleanupReceiptState {
    Pending,
    ObservationRecorded,
    TargetComplete,
    InventoryPageRecorded,
    InventoryComplete,
    Replayed,
}

impl JobsWorkflowCleanupWorkLease {
    fn request_identity(&self) -> (&str, i64, i64, &str, &str, i64) {
        match self {
            Self::LegacyInventoryPage(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                &lease.lease_owner,
                &lease.lease_token,
                lease.lease_expires_at_ms,
            ),
            Self::ReconcileLegacyTarget(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                &lease.lease_owner,
                &lease.lease_token,
                lease.lease_expires_at_ms,
            ),
            Self::ReconcileV2Target(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                &lease.lease_owner,
                &lease.lease_token,
                lease.lease_expires_at_ms,
            ),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobsLegacyInventoryTargetReceiptV3 {
    workflow_id: String,
    run_id: String,
    first_execution_run_id: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobsLegacyInventoryPageReceiptV3 {
    schema_version: i64,
    operation: String,
    cleanup_request_id: String,
    inventory_generation_id: String,
    namespace: String,
    workflow_type: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    page_index: i64,
    predecessor_page_digest: Option<String>,
    page_token: Option<String>,
    cleanup_fence: i64,
    outcome: String,
    page_digest: String,
    targets_digest: String,
    targets: Vec<JobsLegacyInventoryTargetReceiptV3>,
    next_page_token: Option<String>,
    exhausted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobsLegacyTargetReceiptV3 {
    schema_version: i64,
    operation: String,
    cleanup_request_id: String,
    inventory_generation_id: String,
    namespace: String,
    workflow_type: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    workflow_id: String,
    run_id: String,
    first_execution_run_id: String,
    target_digest: String,
    cleanup_fence: i64,
    observation_pass: i64,
    outcome: String,
    reason: String,
    run_ids: Vec<String>,
    evidence_digest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobsV2TargetReceiptV3 {
    schema_version: i64,
    operation: String,
    cleanup_request_id: String,
    cleanup_generation_id: String,
    target_set_digest: String,
    namespace: String,
    workflow_type: String,
    workflow_id: String,
    first_execution_run_id: Option<String>,
    start_request_id: String,
    start_payload_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_cloud_binding_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_cloud_release_memo_base64url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_cloud_release_memo_sha256: Option<String>,
    known_run_ids: Vec<String>,
    target_digest: String,
    cleanup_fence: i64,
    observation_pass: i64,
    outcome: String,
    reason: String,
    run_ids: Vec<String>,
    evidence_digest: String,
}

fn workflow_cleanup_page_token(value: Option<&str>) -> bool {
    value.is_none_or(|token| {
        !token.is_empty()
            && base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(token)
                .ok()
                .filter(|decoded| decoded.len() <= WORKFLOW_LEGACY_TOKEN_MAX_BYTES)
                .is_some_and(|decoded| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(decoded) == token
                })
    })
}

fn workflow_cleanup_sorted_unique_identifiers(values: &[String], maximum: usize) -> bool {
    values.len() <= maximum
        && values
            .iter()
            .all(|value| workflow_command_opaque_identifier(value, 128))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn validate_workflow_cleanup_managed_cloud_memo(
    binding_sha256: Option<&str>,
    memo_base64url: Option<&str>,
    memo_sha256: Option<&str>,
) -> Result<()> {
    let (Some(binding_sha256), Some(memo_base64url), Some(memo_sha256)) =
        (binding_sha256, memo_base64url, memo_sha256)
    else {
        return if binding_sha256.is_none() && memo_base64url.is_none() && memo_sha256.is_none() {
            Ok(())
        } else {
            Err(JobsWorkflowCommandError::InvalidState.into())
        };
    };
    if !workflow_cleanup_digest(binding_sha256)
        || !workflow_cleanup_digest(memo_sha256)
        || memo_base64url.is_empty()
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(memo_base64url)
        .map_err(|_| JobsWorkflowCommandError::InvalidState)?;
    if bytes.is_empty()
        || bytes.len() > WORKFLOW_CLEANUP_MAX_MANAGED_CLOUD_MEMO_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes) != memo_base64url
        || hex::encode(Sha256::digest(&bytes)) != memo_sha256
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    let parsed: ManagedCloudReleaseMemoAuthority =
        serde_json::from_slice(&bytes).map_err(|_| JobsWorkflowCommandError::InvalidState)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| JobsWorkflowCommandError::InvalidState)?;
    let object = value
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidState)?;
    let expected_keys = [
        "activationExpiresAtMs",
        "activationSha256",
        "bindingSha256",
        "channelSequence",
        "cohortSha256",
        "failureConverterSha256",
        "headRevision",
        "manifestSha256",
        "readinessSha256",
        "releaseId",
        "releaseSequence",
        "resolvedAtMs",
        "scope",
        "taskQueueSha256",
        "transitionSha256",
        "trustGeneration",
        "version",
    ];
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
        || parsed.version != 1
        || parsed.execution.binding_sha256 != binding_sha256
        || serde_json::to_vec(&value)? != bytes
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(())
}

fn workflow_cleanup_legacy_workflow_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("bluey-jobs:") else {
        return false;
    };
    if !(18..=412).contains(&value.len())
        || !rest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
    {
        return false;
    }
    rest.bytes().enumerate().any(|(index, byte)| {
        byte == b':' && (3..=200).contains(&index) && (3..=200).contains(&(rest.len() - index - 1))
    })
}

fn workflow_cleanup_evidence_digest(receipt: &Value) -> Result<String> {
    let mut material = receipt.clone();
    let object = material
        .as_object_mut()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    object
        .remove("evidenceDigest")
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_EVIDENCE_DIGEST_DOMAIN,
        &material,
        "workflow cleanup receipt evidence",
    )
}

fn workflow_cleanup_legacy_target_digest(lease: &JobsLegacyTargetCleanupLease) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_TARGET_DIGEST_DOMAIN,
        &json!({
            "firstExecutionRunId": lease.first_execution_run_id,
            "inventoryGenerationId": lease.inventory_generation_id,
            "namespace": lease.namespace,
            "queryDigest": lease.query_digest,
            "runId": lease.run_id,
            "scanPass": lease.scan_pass,
            "visibilityCutoffMs": lease.visibility_cutoff_ms,
            "workflowId": lease.workflow_id,
            "workflowType": WORKFLOW_LEGACY_TYPE,
        }),
        "workflow legacy cleanup target",
    )
}

#[derive(Debug)]
struct StoredLegacyTargetClaim {
    generation: i64,
    inventory_generation_id: String,
    namespace_ciphertext: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    target_identity_hmac: String,
    workflow_id_ciphertext: String,
    run_id_ciphertext: String,
    first_run_id_ciphertext: String,
    target_digest: String,
    observation_pass: i64,
    proof_epoch: i64,
    request_epoch: i64,
    fence: i64,
    request_id: Option<String>,
    first_request_started_at_ms: Option<i64>,
}

#[derive(Debug)]
struct StoredV2TargetClaim {
    account_id: String,
    generation: i64,
    workflow_id: String,
    first_execution_run_id: Option<String>,
    start_request_id: String,
    start_payload_digest: String,
    managed_cloud_binding_sha256: Option<String>,
    managed_cloud_release_memo_base64url: Option<String>,
    managed_cloud_release_memo_sha256: Option<String>,
    target_set_digest: String,
    cleanup_generation_id: String,
    namespace_ciphertext: String,
    known_run_set_digest: String,
    target_digest: String,
    observation_pass: i64,
    request_epoch: i64,
    fence: i64,
    request_id: Option<String>,
    first_request_started_at_ms: Option<i64>,
}

#[derive(Debug)]
struct StoredLegacyPageClaim {
    generation: i64,
    inventory_generation_id: String,
    namespace_ciphertext: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    page_index: i64,
    predecessor_page_digest: String,
    page_token_ciphertext: Option<String>,
    request_epoch: i64,
    fence: i64,
    request_id: Option<String>,
    first_request_started_at_ms: Option<i64>,
}

fn next_cleanup_request_identity(
    request_epoch: i64,
    fence: i64,
    request_id: Option<String>,
    request_started_at_ms: Option<i64>,
) -> Result<(i64, i64, String)> {
    if request_started_at_ms.is_some() {
        let request_id = request_id.ok_or(JobsWorkflowCommandError::InvalidState)?;
        return Ok((request_epoch, fence, request_id));
    }
    Ok((
        request_epoch
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?,
        fence
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?,
        new_jobs_workflow_cleanup_request_id(),
    ))
}

#[derive(Debug, Clone)]
pub struct PrepareJobsLegacyInventoryGeneration {
    pub namespace: String,
    pub visibility_cutoff_ms: i64,
    pub confirmation_age_ms: i64,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsLegacyInventoryAuthorityRef {
    pub inventory_generation_id: String,
    pub query_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobsLegacyInventoryState {
    Scanning,
    Draining,
    AwaitingSecondScan,
    Complete,
    IdentityConflict,
}

impl JobsLegacyInventoryState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "scanning" => Ok(Self::Scanning),
            "draining" => Ok(Self::Draining),
            "awaiting_second_scan" => Ok(Self::AwaitingSecondScan),
            "complete" => Ok(Self::Complete),
            "identity_conflict" => Ok(Self::IdentityConflict),
            _ => anyhow::bail!("invalid stored workflow legacy inventory state"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsLegacyInventoryGeneration {
    pub authority: JobsLegacyInventoryAuthorityRef,
    pub storage_generation: i64,
    pub namespace: String,
    pub workflow_type: String,
    pub visibility_cutoff_ms: i64,
    pub confirmation_age_ms: i64,
    pub visibility_query: String,
    pub state: JobsLegacyInventoryState,
    pub scan_pass: i64,
    pub page_index: i64,
    pub completion_epoch: i64,
    pub completed_at_ms: Option<i64>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCleanupCompletionTombstone {
    pub tombstone_id: String,
    pub account_generation: i64,
    pub workflow_cleanup_generation: i64,
    pub cleanup_generation_id: String,
    pub target_set_digest: String,
    pub legacy_generation: i64,
    pub legacy_inventory_generation_id: String,
    pub legacy_completion_epoch: i64,
    pub legacy_completion_digest: String,
    pub legacy_revalidate_after_ms: i64,
    pub completion_digest: String,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCleanupDeletionProof {
    pub account_generation: i64,
    pub cleanup_generation_id: String,
    pub target_set_digest: String,
    pub legacy_authority: JobsLegacyInventoryAuthorityRef,
    pub tombstone_id: String,
    pub completion_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCleanupDeletionStatus {
    pub account_id: String,
    pub account_generation: i64,
    pub workflow_cleanup_generation: i64,
    pub cleanup_generation_id: String,
    pub target_set_digest: String,
    pub legacy_authority: JobsLegacyInventoryAuthorityRef,
    pub v2_target_count: i64,
    pub v2_pending_count: i64,
    pub legacy_pending_count: i64,
    pub legacy_complete: bool,
    pub object_sweep_started_at_ms: Option<i64>,
    pub object_sweep_deleted_count: i64,
    pub object_sweep_orphan_count: i64,
    pub complete: bool,
    pub tombstone: Option<JobsWorkflowCleanupCompletionTombstone>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCleanupObjectSweepScope {
    pub account_id: String,
    pub account_generation: i64,
    pub sweep_attempt_id: String,
    pub scope_id: String,
    pub deleted_count: i64,
    pub orphan_count: i64,
    pub result_digest: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCleanupObjectSweepScopeManifest {
    pub scope_id: String,
    pub object_keys: Vec<String>,
    pub prefix_sweep: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", content = "status", rename_all = "snake_case")]
pub enum JobsWorkflowCleanupObjectSweepBeginResult {
    Authorized(JobsWorkflowCleanupDeletionStatus),
    PendingWorkflow(JobsWorkflowCleanupDeletionStatus),
    PendingRunner(JobsWorkflowCleanupDeletionStatus),
}

#[derive(Debug)]
struct PreparedObjectSweepScope {
    scope_id: String,
    object_key_hmacs: Vec<String>,
    prefix_sweep: bool,
    manifest_digest: String,
}

#[derive(Debug)]
struct StoredRunnerSweepAuthority {
    purge_request_id: String,
    purge_generation: i64,
    tombstone_generation: i64,
    legacy_inventory_generation: i64,
    legacy_reconciliation_id: String,
    legacy_authority_id: String,
    legacy_authority_sha256: String,
    authority_digest: String,
}

struct V2TargetAuthorityInit<'a> {
    account_id: &'a str,
    generation: i64,
    cleanup_generation_id: &'a str,
    target_set_digest: &'a str,
    namespace: &'a str,
    now_ms: i64,
}

struct RunnerSweepAuthorityMaterial {
    purge_request_id: String,
    purge_subject: String,
    purge_generation: i64,
    tombstone_generation: i64,
    target_set_digest: String,
    required_target_count: i64,
    completed_at_ms: i64,
    legacy_inventory_generation: i64,
    legacy_reconciliation_id: String,
    legacy_authority_id: String,
    legacy_authority_sha256: String,
}

struct SweepAuthorizationDigestMaterial<'a> {
    account_id: &'a str,
    requested_at_ms: i64,
    sweep_attempt_id: &'a str,
    scope_count: usize,
    known_object_count: i64,
    scope_set_digest: &'a str,
    runner: &'a StoredRunnerSweepAuthority,
    proof: &'a JobsWorkflowCleanupDeletionProof,
}

impl JobsWorkflowCleanupDeletionStatus {
    pub fn deletion_proof(&self) -> Option<JobsWorkflowCleanupDeletionProof> {
        let tombstone = self.tombstone.as_ref()?;
        self.complete.then(|| JobsWorkflowCleanupDeletionProof {
            account_generation: self.account_generation,
            cleanup_generation_id: self.cleanup_generation_id.clone(),
            target_set_digest: self.target_set_digest.clone(),
            legacy_authority: self.legacy_authority.clone(),
            tombstone_id: tombstone.tombstone_id.clone(),
            completion_digest: tombstone.completion_digest.clone(),
        })
    }
}

fn workflow_cleanup_sha256(domain: &[u8], value: &Value, label: &str) -> Result<String> {
    let canonical =
        canonical_workflow_command_bytes(value, WORKFLOW_COMMAND_REQUEST_MAX_BYTES, label)?;
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(canonical);
    Ok(hex::encode(digest.finalize()))
}

fn workflow_cleanup_visibility_query(_cutoff_ms: i64) -> Result<String> {
    Ok(format!("WorkflowType = \"{WORKFLOW_LEGACY_TYPE}\""))
}

fn workflow_cleanup_query_digest(
    namespace: &str,
    visibility_cutoff_ms: i64,
    query: &str,
) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_QUERY_DIGEST_DOMAIN,
        &json!({
            "namespace": namespace,
            "query": query,
            "visibilityCutoffMs": visibility_cutoff_ms,
            "workflowType": WORKFLOW_LEGACY_TYPE,
        }),
        "workflow legacy inventory query",
    )
}

fn workflow_cleanup_genesis_digest(query_digest: &str, completion_epoch: i64) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_GENESIS_DOMAIN,
        &json!({
            "completionEpoch": completion_epoch,
            "queryDigest": query_digest,
        }),
        "workflow legacy inventory genesis",
    )
}

fn validate_prepare_jobs_legacy_inventory(
    input: &PrepareJobsLegacyInventoryGeneration,
) -> Result<(String, String, String)> {
    if !workflow_command_identifier(&input.namespace, 255)
        || !(0..=253_402_300_799_999).contains(&input.visibility_cutoff_ms)
        || !(1_000..=600_000).contains(&input.confirmation_age_ms)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let query = workflow_cleanup_visibility_query(input.visibility_cutoff_ms)?;
    let query_digest =
        workflow_cleanup_query_digest(&input.namespace, input.visibility_cutoff_ms, &query)?;
    let query_hmac = workflow_command_hmac(
        "legacy-inventory-query-index-v3",
        &json!({
            "namespace": input.namespace,
            "query": query,
            "visibilityCutoffMs": input.visibility_cutoff_ms,
            "workflowType": WORKFLOW_LEGACY_TYPE,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )?;
    Ok((query, query_digest, query_hmac))
}

fn legacy_generation_from_sqlite_row(
    row: &rusqlite::Row<'_>,
    replayed: bool,
) -> rusqlite::Result<JobsLegacyInventoryGeneration> {
    let namespace_ciphertext: String = row.get(2)?;
    let query_ciphertext: String = row.get(6)?;
    let state: String = row.get(8)?;
    Ok(JobsLegacyInventoryGeneration {
        authority: JobsLegacyInventoryAuthorityRef {
            inventory_generation_id: row.get(1)?,
            query_digest: row.get(7)?,
        },
        storage_generation: row.get(0)?,
        namespace: decrypt_payload(&namespace_ciphertext).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, error.into())
        })?,
        workflow_type: row.get(5)?,
        visibility_cutoff_ms: row.get(3)?,
        confirmation_age_ms: row.get(4)?,
        visibility_query: decrypt_payload(&query_ciphertext).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, error.into())
        })?,
        state: JobsLegacyInventoryState::parse(&state).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, error.into())
        })?,
        scan_pass: row.get(9)?,
        page_index: row.get(10)?,
        completion_epoch: row.get(11)?,
        completed_at_ms: row.get(12)?,
        replayed,
    })
}

const LEGACY_GENERATION_STATUS_SELECT: &str =
    "generation, inventory_generation_id, namespace_ciphertext, visibility_cutoff_ms,
     confirmation_age_ms, workflow_type, visibility_query_ciphertext,
     query_digest_sha256, state, scan_pass, page_index, completion_epoch, completed_at_ms";
const LEGACY_GENERATION_STATUS_SELECT_QUALIFIED: &str =
    "generation.generation, generation.inventory_generation_id,
     generation.namespace_ciphertext, generation.visibility_cutoff_ms,
     generation.confirmation_age_ms, generation.workflow_type,
     generation.visibility_query_ciphertext, generation.query_digest_sha256,
     generation.state, generation.scan_pass, generation.page_index,
     generation.completion_epoch, generation.completed_at_ms";

fn legacy_generation_from_postgres_row(
    row: &postgres::Row,
    replayed: bool,
) -> Result<JobsLegacyInventoryGeneration> {
    let namespace_ciphertext: String = row.get(2);
    let query_ciphertext: String = row.get(6);
    Ok(JobsLegacyInventoryGeneration {
        authority: JobsLegacyInventoryAuthorityRef {
            inventory_generation_id: row.get(1),
            query_digest: row.get(7),
        },
        storage_generation: row.get(0),
        namespace: decrypt_payload(&namespace_ciphertext)?,
        workflow_type: row.get(5),
        visibility_cutoff_ms: row.get(3),
        confirmation_age_ms: row.get(4),
        visibility_query: decrypt_payload(&query_ciphertext)?,
        state: JobsLegacyInventoryState::parse(row.get::<_, String>(8).as_str())?,
        scan_pass: row.get(9),
        page_index: row.get(10),
        completion_epoch: row.get(11),
        completed_at_ms: row.get(12),
        replayed,
    })
}

pub fn prepare_jobs_legacy_inventory_generation(
    pool: &DbPool,
    input: &PrepareJobsLegacyInventoryGeneration,
) -> Result<JobsLegacyInventoryGeneration> {
    let (query, query_digest, query_hmac) = validate_prepare_jobs_legacy_inventory(input)?;
    let namespace_hmac = workflow_command_hmac(
        "legacy-inventory-namespace-index-v3",
        &json!({"namespace": input.namespace}),
        1_024,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let authority_now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            if let Some(existing) = tx
                .query_row(
                    &format!(
                        "SELECT {LEGACY_GENERATION_STATUS_SELECT_QUALIFIED}
                           FROM jobs_workflow_legacy_inventory_generations generation
                           JOIN jobs_workflow_legacy_inventory_head head
                             ON head.generation = generation.generation
                            AND head.query_digest_sha256 = generation.query_digest_sha256
                          WHERE head.singleton_id = 1"
                    ),
                    [],
                    |row| legacy_generation_from_sqlite_row(row, true),
                )
                .optional()?
            {
                if existing.namespace != input.namespace
                    || existing.workflow_type != WORKFLOW_LEGACY_TYPE
                    || existing.visibility_cutoff_ms != input.visibility_cutoff_ms
                    || existing.confirmation_age_ms != input.confirmation_age_ms
                    || existing.visibility_query != query
                    || !workflow_command_hmac_matches(
                        &existing.authority.query_digest,
                        &query_digest,
                    )
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(existing);
            }
            let generation: i64 = tx.query_row(
                "SELECT COALESCE(MAX(generation), 0) + 1
                   FROM jobs_workflow_legacy_inventory_generations",
                [],
                |row| row.get(0),
            )?;
            let inventory_generation_id = format!("wfinventory-v3-{}", uuid::Uuid::new_v4());
            let completion_epoch = 1_i64;
            let genesis = workflow_cleanup_genesis_digest(&query_digest, completion_epoch)?;
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_inventory_generations (
                    generation, inventory_generation_id, namespace_ciphertext,
                    namespace_hmac_sha256, workflow_type, visibility_cutoff_ms,
                    confirmation_age_ms, visibility_query_ciphertext, query_digest_sha256,
                    query_hmac_sha256, state, scan_pass, page_index,
                    predecessor_page_digest_sha256, next_attempt_at_ms, completion_epoch,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'scanning', 1, 0,
                    ?11, ?12, ?13, ?14, ?14)",
                params![
                    generation,
                    inventory_generation_id,
                    encrypt_payload(&input.namespace)?,
                    namespace_hmac,
                    WORKFLOW_LEGACY_TYPE,
                    input.visibility_cutoff_ms,
                    input.confirmation_age_ms,
                    encrypt_payload(&query)?,
                    query_digest,
                    query_hmac,
                    genesis,
                    authority_now_ms,
                    completion_epoch,
                    authority_now_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_inventory_head (
                    singleton_id, generation, inventory_generation_id,
                    query_digest_sha256, updated_at_ms
                 ) VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    generation,
                    inventory_generation_id,
                    query_digest,
                    authority_now_ms
                ],
            )?;
            let result = tx.query_row(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT}
                       FROM jobs_workflow_legacy_inventory_generations
                      WHERE generation = ?1"
                ),
                params![generation],
                |row| legacy_generation_from_sqlite_row(row, false),
            )?;
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let authority_now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended(
                    'jobs-workflow-legacy-inventory-head', 0))",
                &[],
            )?;
            if let Some(row) = tx.query_opt(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT_QUALIFIED}
                       FROM jobs_workflow_legacy_inventory_generations generation
                       JOIN jobs_workflow_legacy_inventory_head head
                         ON head.generation = generation.generation
                        AND head.query_digest_sha256 = generation.query_digest_sha256
                      WHERE head.singleton_id = 1 FOR UPDATE OF generation, head"
                ),
                &[],
            )? {
                let existing = legacy_generation_from_postgres_row(&row, true)?;
                if existing.namespace != input.namespace
                    || existing.workflow_type != WORKFLOW_LEGACY_TYPE
                    || existing.visibility_cutoff_ms != input.visibility_cutoff_ms
                    || existing.confirmation_age_ms != input.confirmation_age_ms
                    || existing.visibility_query != query
                    || !workflow_command_hmac_matches(
                        &existing.authority.query_digest,
                        &query_digest,
                    )
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(existing);
            }
            let generation: i64 = tx
                .query_one(
                    "SELECT COALESCE(MAX(generation), 0)::bigint + 1
                       FROM jobs_workflow_legacy_inventory_generations",
                    &[],
                )?
                .get(0);
            let inventory_generation_id = format!("wfinventory-v3-{}", uuid::Uuid::new_v4());
            let completion_epoch = 1_i64;
            let genesis = workflow_cleanup_genesis_digest(&query_digest, completion_epoch)?;
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_inventory_generations (
                    generation, inventory_generation_id, namespace_ciphertext,
                    namespace_hmac_sha256, workflow_type, visibility_cutoff_ms,
                    confirmation_age_ms, visibility_query_ciphertext, query_digest_sha256,
                    query_hmac_sha256, state, scan_pass, page_index,
                    predecessor_page_digest_sha256, next_attempt_at_ms, completion_epoch,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'scanning', 1, 0,
                    $11, $12, $13, $14, $14)",
                &[
                    &generation,
                    &inventory_generation_id,
                    &encrypt_payload(&input.namespace)?,
                    &namespace_hmac,
                    &WORKFLOW_LEGACY_TYPE,
                    &input.visibility_cutoff_ms,
                    &input.confirmation_age_ms,
                    &encrypt_payload(&query)?,
                    &query_digest,
                    &query_hmac,
                    &genesis,
                    &authority_now_ms,
                    &completion_epoch,
                    &authority_now_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_inventory_head (
                    singleton_id, generation, inventory_generation_id,
                    query_digest_sha256, updated_at_ms
                 ) VALUES (1, $1, $2, $3, $4)",
                &[
                    &generation,
                    &inventory_generation_id,
                    &query_digest,
                    &authority_now_ms,
                ],
            )?;
            let row = tx.query_one(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT}
                       FROM jobs_workflow_legacy_inventory_generations
                      WHERE generation = $1"
                ),
                &[&generation],
            )?;
            let result = legacy_generation_from_postgres_row(&row, false)?;
            tx.commit()?;
            Ok(result)
        }
    })
}

pub fn current_jobs_legacy_inventory_authority(
    pool: &DbPool,
) -> Result<Option<JobsLegacyInventoryAuthorityRef>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let connection = pool.get()?;
            Ok(connection
                .query_row(
                    "SELECT inventory_generation_id, query_digest_sha256
                       FROM jobs_workflow_legacy_inventory_head WHERE singleton_id = 1",
                    [],
                    |row| {
                        Ok(JobsLegacyInventoryAuthorityRef {
                            inventory_generation_id: row.get(0)?,
                            query_digest: row.get(1)?,
                        })
                    },
                )
                .optional()?)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            Ok(connection
                .query_opt(
                    "SELECT inventory_generation_id, query_digest_sha256
                       FROM jobs_workflow_legacy_inventory_head WHERE singleton_id = 1",
                    &[],
                )?
                .map(|row| JobsLegacyInventoryAuthorityRef {
                    inventory_generation_id: row.get(0),
                    query_digest: row.get(1),
                }))
        }
    })
}

/// Requests an immediate fresh global legacy scan using the exact current
/// authority. This operation can only make cleanup stricter: a completed
/// generation is reopened at a successor completion epoch using database time;
/// an already-incomplete generation is returned unchanged.
pub fn request_jobs_legacy_inventory_revalidation(
    pool: &DbPool,
    authority: &JobsLegacyInventoryAuthorityRef,
    caller_now_ms: i64,
) -> Result<JobsLegacyInventoryGeneration> {
    if !workflow_command_opaque_identifier(&authority.inventory_generation_id, 128)
        || !workflow_cleanup_digest(&authority.query_digest)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&caller_now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = tx.query_row(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT_QUALIFIED}
                       FROM jobs_workflow_legacy_inventory_generations generation
                       JOIN jobs_workflow_legacy_inventory_head head
                         ON head.generation = generation.generation
                        AND head.inventory_generation_id = generation.inventory_generation_id
                        AND head.query_digest_sha256 = generation.query_digest_sha256
                      WHERE head.singleton_id = 1"
                ),
                [],
                |row| legacy_generation_from_sqlite_row(row, true),
            )?;
            if current.authority != *authority {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let reopened = current.state == JobsLegacyInventoryState::Complete;
            if reopened {
                let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
                let next_epoch = current
                    .completion_epoch
                    .checked_add(1)
                    .ok_or(JobsWorkflowCommandError::InvalidState)?;
                let genesis = workflow_cleanup_genesis_digest(&authority.query_digest, next_epoch)?;
                if tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = 'scanning', scan_pass = 1, page_index = 0,
                            predecessor_page_digest_sha256 = ?1,
                            page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                            request_epoch = 0, fence = 0, request_id = NULL,
                            first_request_started_at_ms = NULL, last_outcome_code = NULL,
                            lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL, next_attempt_at_ms = ?2,
                            completion_epoch = ?3, first_zero_observed_at_ms = NULL,
                            completed_at_ms = NULL, completion_digest_sha256 = NULL,
                            revalidate_after_ms = NULL,
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE generation = ?4 AND state = 'complete'
                        AND completion_epoch = ?5",
                    params![
                        genesis,
                        now_ms,
                        next_epoch,
                        current.storage_generation,
                        current.completion_epoch,
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            let result = tx.query_row(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT}
                       FROM jobs_workflow_legacy_inventory_generations
                      WHERE generation = ?1"
                ),
                params![current.storage_generation],
                |row| legacy_generation_from_sqlite_row(row, !reopened),
            )?;
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let row = tx.query_one(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT_QUALIFIED}
                       FROM jobs_workflow_legacy_inventory_generations generation
                       JOIN jobs_workflow_legacy_inventory_head head
                         ON head.generation = generation.generation
                        AND head.inventory_generation_id = generation.inventory_generation_id
                        AND head.query_digest_sha256 = generation.query_digest_sha256
                      WHERE head.singleton_id = 1 FOR UPDATE OF head, generation"
                ),
                &[],
            )?;
            let current = legacy_generation_from_postgres_row(&row, true)?;
            if current.authority != *authority {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let reopened = current.state == JobsLegacyInventoryState::Complete;
            if reopened {
                let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
                let next_epoch = current
                    .completion_epoch
                    .checked_add(1)
                    .ok_or(JobsWorkflowCommandError::InvalidState)?;
                let genesis = workflow_cleanup_genesis_digest(&authority.query_digest, next_epoch)?;
                if tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = 'scanning', scan_pass = 1, page_index = 0,
                            predecessor_page_digest_sha256 = $1,
                            page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                            request_epoch = 0, fence = 0, request_id = NULL,
                            first_request_started_at_ms = NULL, last_outcome_code = NULL,
                            lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL, next_attempt_at_ms = $2,
                            completion_epoch = $3, first_zero_observed_at_ms = NULL,
                            completed_at_ms = NULL, completion_digest_sha256 = NULL,
                            revalidate_after_ms = NULL,
                            updated_at_ms = GREATEST($2, updated_at_ms + 1)
                      WHERE generation = $4 AND state = 'complete'
                        AND completion_epoch = $5",
                    &[
                        &genesis,
                        &now_ms,
                        &next_epoch,
                        &current.storage_generation,
                        &current.completion_epoch,
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            let row = tx.query_one(
                &format!(
                    "SELECT {LEGACY_GENERATION_STATUS_SELECT}
                       FROM jobs_workflow_legacy_inventory_generations
                      WHERE generation = $1"
                ),
                &[&current.storage_generation],
            )?;
            let result = legacy_generation_from_postgres_row(&row, !reopened)?;
            tx.commit()?;
            Ok(result)
        }
    })
}

fn advance_jobs_legacy_inventory_state_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    now_ms: i64,
) -> Result<()> {
    let current = tx
        .query_row(
            "SELECT legacy.generation, legacy.state, legacy.completion_epoch,
                    legacy.query_digest_sha256, legacy.first_zero_observed_at_ms,
                    legacy.confirmation_age_ms, legacy.revalidate_after_ms
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some(current) = current else {
        return Ok(());
    };
    if current.1 == "complete" && current.6.is_some_and(|due| due <= now_ms) {
        let next_epoch = current
            .2
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let genesis = workflow_cleanup_genesis_digest(&current.3, next_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 1, page_index = 0,
                    predecessor_page_digest_sha256 = ?1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL, last_outcome_code = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, next_attempt_at_ms = ?2,
                    completion_epoch = ?3, first_zero_observed_at_ms = NULL,
                    completed_at_ms = NULL, completion_digest_sha256 = NULL,
                    revalidate_after_ms = NULL, updated_at_ms = MAX(?2, updated_at_ms + 1)
              WHERE generation = ?4 AND state = 'complete' AND completion_epoch = ?5",
            params![genesis, now_ms, next_epoch, current.0, current.2],
        )?;
    } else if current.1 == "awaiting_second_scan"
        && current
            .4
            .is_some_and(|first| first.saturating_add(current.5) <= now_ms)
    {
        let genesis = workflow_cleanup_genesis_digest(&current.3, current.2)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 2, page_index = 0,
                    predecessor_page_digest_sha256 = ?1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL, last_outcome_code = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, next_attempt_at_ms = ?2,
                    first_zero_observed_at_ms = NULL,
                    updated_at_ms = MAX(?2, updated_at_ms + 1)
              WHERE generation = ?3 AND state = 'awaiting_second_scan'",
            params![genesis, now_ms, current.0],
        )?;
    } else if current.1 == "draining" {
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', next_attempt_at_ms = ?1,
                    updated_at_ms = MAX(?1, updated_at_ms + 1)
              WHERE generation = ?2 AND state = 'draining'
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_targets target
                   WHERE target.generation = ?2 AND target.target_state <> 'absence_proved'
                )",
            params![now_ms, current.0],
        )?;
    }
    Ok(())
}

fn advance_jobs_legacy_inventory_state_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    now_ms: i64,
) -> Result<()> {
    let current = tx.query_opt(
        "SELECT legacy.generation, legacy.state, legacy.completion_epoch,
                legacy.query_digest_sha256, legacy.first_zero_observed_at_ms,
                legacy.confirmation_age_ms, legacy.revalidate_after_ms
           FROM jobs_workflow_legacy_inventory_head head
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = head.generation
            AND legacy.query_digest_sha256 = head.query_digest_sha256
          WHERE head.singleton_id = 1 FOR UPDATE OF head, legacy",
        &[],
    )?;
    let Some(current) = current else {
        return Ok(());
    };
    let generation: i64 = current.get(0);
    let state: String = current.get(1);
    let completion_epoch: i64 = current.get(2);
    let query_digest: String = current.get(3);
    let first_zero: Option<i64> = current.get(4);
    let confirmation_age: i64 = current.get(5);
    let revalidate_after: Option<i64> = current.get(6);
    if state == "complete" && revalidate_after.is_some_and(|due| due <= now_ms) {
        let next_epoch = completion_epoch
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let genesis = workflow_cleanup_genesis_digest(&query_digest, next_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 1, page_index = 0,
                    predecessor_page_digest_sha256 = $1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL, last_outcome_code = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, next_attempt_at_ms = $2,
                    completion_epoch = $3, first_zero_observed_at_ms = NULL,
                    completed_at_ms = NULL, completion_digest_sha256 = NULL,
                    revalidate_after_ms = NULL,
                    updated_at_ms = GREATEST($2, updated_at_ms + 1)
              WHERE generation = $4 AND state = 'complete' AND completion_epoch = $5",
            &[
                &genesis,
                &now_ms,
                &next_epoch,
                &generation,
                &completion_epoch,
            ],
        )?;
    } else if state == "awaiting_second_scan"
        && first_zero.is_some_and(|first| first.saturating_add(confirmation_age) <= now_ms)
    {
        let genesis = workflow_cleanup_genesis_digest(&query_digest, completion_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 2, page_index = 0,
                    predecessor_page_digest_sha256 = $1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL, last_outcome_code = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, next_attempt_at_ms = $2,
                    first_zero_observed_at_ms = NULL,
                    updated_at_ms = GREATEST($2, updated_at_ms + 1)
              WHERE generation = $3 AND state = 'awaiting_second_scan'",
            &[&genesis, &now_ms, &generation],
        )?;
    } else if state == "draining" {
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', next_attempt_at_ms = $1,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE generation = $2 AND state = 'draining'
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_targets target
                   WHERE target.generation = $2 AND target.target_state <> 'absence_proved'
                )",
            &[&now_ms, &generation],
        )?;
    }
    Ok(())
}

pub fn claim_jobs_workflow_cleanup_work(
    pool: &DbPool,
    owner_id: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<Option<JobsWorkflowCleanupWorkLease>> {
    validate_workflow_command_lease_input(owner_id, now_ms, lease_ms)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let authority_now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let lease_expires_at_ms = authority_now_ms
                .checked_add(lease_ms)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            advance_jobs_legacy_inventory_state_sqlite_tx(&tx, authority_now_ms)?;
            if let Some(lease) = claim_jobs_legacy_target_sqlite_tx(
                &tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )? {
                tx.commit()?;
                return Ok(Some(JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(
                    lease,
                )));
            }
            if let Some(lease) = claim_jobs_v2_target_sqlite_tx(
                &tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )? {
                tx.commit()?;
                return Ok(Some(JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease)));
            }
            let lease = claim_jobs_legacy_page_sqlite_tx(
                &tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )?;
            tx.commit()?;
            Ok(lease.map(JobsWorkflowCleanupWorkLease::LegacyInventoryPage))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let authority_now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            let lease_expires_at_ms = authority_now_ms
                .checked_add(lease_ms)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            advance_jobs_legacy_inventory_state_postgres_tx(&mut tx, authority_now_ms)?;
            if let Some(lease) = claim_jobs_legacy_target_postgres_tx(
                &mut tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )? {
                tx.commit()?;
                return Ok(Some(JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(
                    lease,
                )));
            }
            if let Some(lease) = claim_jobs_v2_target_postgres_tx(
                &mut tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )? {
                tx.commit()?;
                return Ok(Some(JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease)));
            }
            let lease = claim_jobs_legacy_page_postgres_tx(
                &mut tx,
                owner_id,
                authority_now_ms,
                lease_expires_at_ms,
            )?;
            tx.commit()?;
            Ok(lease.map(JobsWorkflowCleanupWorkLease::LegacyInventoryPage))
        }
    })
}

fn claim_jobs_legacy_target_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsLegacyTargetCleanupLease>> {
    let stored = tx
        .query_row(
            "SELECT target.generation, legacy.inventory_generation_id,
                    legacy.namespace_ciphertext, legacy.visibility_cutoff_ms,
                    legacy.query_digest_sha256, target.discovered_scan_pass,
                    target.target_identity_hmac_sha256, target.workflow_id_ciphertext,
                    target.run_id_ciphertext, target.first_execution_run_id_ciphertext,
                    target.target_digest_sha256, target.observation_pass,
                    target.proof_epoch, target.request_epoch, target.fence, target.request_id,
                    target.first_request_started_at_ms
               FROM jobs_workflow_legacy_targets target
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = target.generation
               JOIN jobs_workflow_legacy_inventory_head head
                 ON head.generation = legacy.generation
                AND head.query_digest_sha256 = legacy.query_digest_sha256
              WHERE legacy.state = 'draining'
                AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
                AND target.raw_ids_scrubbed = 0
                AND target.positive_reset_required = 0
                AND target.next_attempt_at_ms <= ?1
                AND (target.lease_owner IS NULL OR target.lease_expires_at_ms <= ?1)
              ORDER BY target.updated_at_ms, target.generation,
                       target.target_identity_hmac_sha256 LIMIT 1",
            params![now_ms],
            |row| {
                Ok(StoredLegacyTargetClaim {
                    generation: row.get(0)?,
                    inventory_generation_id: row.get(1)?,
                    namespace_ciphertext: row.get(2)?,
                    visibility_cutoff_ms: row.get(3)?,
                    query_digest: row.get(4)?,
                    scan_pass: row.get(5)?,
                    target_identity_hmac: row.get(6)?,
                    workflow_id_ciphertext: row.get(7)?,
                    run_id_ciphertext: row.get(8)?,
                    first_run_id_ciphertext: row.get(9)?,
                    target_digest: row.get(10)?,
                    observation_pass: row.get(11)?,
                    proof_epoch: row.get(12)?,
                    request_epoch: row.get(13)?,
                    fence: row.get(14)?,
                    request_id: row.get(15)?,
                    first_request_started_at_ms: row.get(16)?,
                })
            },
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_legacy_targets
            SET request_epoch = ?1, fence = ?2, request_id = ?3,
                first_request_started_at_ms = CASE WHEN ?4 = 1
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = ?5,
                lease_token_sha256 = ?6, lease_expires_at_ms = ?7,
                updated_at_ms = MAX(?8, updated_at_ms + 1)
          WHERE generation = ?9 AND target_identity_hmac_sha256 = ?10
            AND proof_epoch = ?11 AND request_epoch = ?12 AND fence = ?13
            AND (lease_owner IS NULL OR lease_expires_at_ms <= ?8)",
        params![
            request_epoch,
            fence,
            cleanup_request_id,
            i64::from(reuse_started_request),
            owner_id,
            lease_hash,
            lease_expires_at_ms,
            now_ms,
            stored.generation,
            stored.target_identity_hmac,
            stored.proof_epoch,
            stored.request_epoch,
            stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow legacy cleanup target changed while being claimed")
    }
    let lease = JobsLegacyTargetCleanupLease {
        cleanup_request_id,
        inventory_generation_id: stored.inventory_generation_id,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
        visibility_cutoff_ms: stored.visibility_cutoff_ms,
        query_digest: stored.query_digest,
        scan_pass: stored.scan_pass,
        workflow_id: decrypt_payload(&stored.workflow_id_ciphertext)?,
        run_id: decrypt_payload(&stored.run_id_ciphertext)?,
        first_execution_run_id: decrypt_payload(&stored.first_run_id_ciphertext)?,
        target_digest: stored.target_digest,
        cleanup_fence: fence,
        observation_pass: stored.observation_pass,
        proof_epoch: stored.proof_epoch,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    };
    if !workflow_cleanup_legacy_workflow_id(&lease.workflow_id)
        || !workflow_command_opaque_identifier(&lease.run_id, 128)
        || !workflow_command_opaque_identifier(&lease.first_execution_run_id, 128)
        || workflow_cleanup_legacy_target_digest(&lease)? != lease.target_digest
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(Some(lease))
}

fn claim_jobs_legacy_page_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsLegacyInventoryPageLease>> {
    let stored = tx
        .query_row(
            "SELECT legacy.generation, legacy.inventory_generation_id,
                    legacy.namespace_ciphertext, legacy.visibility_cutoff_ms,
                    legacy.query_digest_sha256, legacy.scan_pass, legacy.page_index,
                    legacy.predecessor_page_digest_sha256, legacy.page_token_ciphertext,
                    legacy.request_epoch, legacy.fence, legacy.request_id,
                    legacy.first_request_started_at_ms
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1 AND legacy.state = 'scanning'
                AND legacy.next_attempt_at_ms <= ?1
                AND (legacy.lease_owner IS NULL OR legacy.lease_expires_at_ms <= ?1)",
            params![now_ms],
            |row| {
                Ok(StoredLegacyPageClaim {
                    generation: row.get(0)?,
                    inventory_generation_id: row.get(1)?,
                    namespace_ciphertext: row.get(2)?,
                    visibility_cutoff_ms: row.get(3)?,
                    query_digest: row.get(4)?,
                    scan_pass: row.get(5)?,
                    page_index: row.get(6)?,
                    predecessor_page_digest: row.get(7)?,
                    page_token_ciphertext: row.get(8)?,
                    request_epoch: row.get(9)?,
                    fence: row.get(10)?,
                    request_id: row.get(11)?,
                    first_request_started_at_ms: row.get(12)?,
                })
            },
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_legacy_inventory_generations
            SET request_epoch = ?1, fence = ?2, request_id = ?3,
                first_request_started_at_ms = CASE WHEN ?4 = 1
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = ?5,
                lease_token_sha256 = ?6, lease_expires_at_ms = ?7,
                updated_at_ms = MAX(?8, updated_at_ms + 1)
          WHERE generation = ?9 AND request_epoch = ?10 AND fence = ?11
            AND state = 'scanning'
            AND (lease_owner IS NULL OR lease_expires_at_ms <= ?8)",
        params![
            request_epoch,
            fence,
            cleanup_request_id,
            i64::from(reuse_started_request),
            owner_id,
            lease_hash,
            lease_expires_at_ms,
            now_ms,
            stored.generation,
            stored.request_epoch,
            stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow legacy inventory page changed while being claimed")
    }
    let page_token = stored
        .page_token_ciphertext
        .as_deref()
        .map(decrypt_payload)
        .transpose()?;
    if !workflow_cleanup_page_token(page_token.as_deref()) {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(Some(JobsLegacyInventoryPageLease {
        cleanup_request_id,
        inventory_generation_id: stored.inventory_generation_id,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
        visibility_cutoff_ms: stored.visibility_cutoff_ms,
        query_digest: stored.query_digest,
        scan_pass: stored.scan_pass,
        page_index: stored.page_index,
        predecessor_page_digest: (stored.page_index > 0).then_some(stored.predecessor_page_digest),
        page_token,
        cleanup_fence: fence,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    }))
}

fn claim_jobs_v2_target_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsV2TargetCleanupLease>> {
    let stored = tx
        .query_row(
            "SELECT target.account_id, target.generation, target.workflow_id,
                    target.first_execution_run_id, target.start_request_id,
                    target.start_payload_hmac_sha256,
                    target.managed_cloud_binding_sha256,
                    target.managed_cloud_release_memo_base64url,
                    target.managed_cloud_release_memo_sha256,
                    target.target_set_hmac_sha256,
                    binding.cleanup_generation_id, legacy.namespace_ciphertext,
                    authority.known_run_set_digest_sha256,
                    authority.target_digest_sha256, authority.observation_pass,
                    authority.request_epoch, authority.cleanup_fence,
                    authority.cleanup_request_id, authority.first_request_started_at_ms
               FROM jobs_workflow_cleanup_v2_target_authorities authority
               JOIN jobs_workflow_cleanup_targets target
                 ON target.account_id = authority.account_id
                AND target.generation = authority.workflow_cleanup_generation
                AND target.workflow_id = authority.workflow_id
               JOIN jobs_workflow_cleanup_account_bindings binding
                 ON binding.account_id = target.account_id
                AND binding.workflow_cleanup_generation = target.generation
                AND binding.target_set_hmac_sha256 = target.target_set_hmac_sha256
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = binding.legacy_generation
                AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
              WHERE binding.state IN ('frozen', 'draining')
                AND authority.positive_reset_required = 0
                AND target.target_state NOT IN (
                  'delivery_drain', 'absence_proved', 'identity_conflict'
                )
                AND authority.next_attempt_at_ms <= ?1
                AND (authority.lease_owner IS NULL OR authority.lease_expires_at_ms <= ?1)
              ORDER BY authority.updated_at_ms, authority.account_id,
                       authority.workflow_id LIMIT 1",
            params![now_ms],
            |row| {
                Ok(StoredV2TargetClaim {
                    account_id: row.get(0)?,
                    generation: row.get(1)?,
                    workflow_id: row.get(2)?,
                    first_execution_run_id: row.get(3)?,
                    start_request_id: row.get(4)?,
                    start_payload_digest: row.get(5)?,
                    managed_cloud_binding_sha256: row.get(6)?,
                    managed_cloud_release_memo_base64url: row.get(7)?,
                    managed_cloud_release_memo_sha256: row.get(8)?,
                    target_set_digest: row.get(9)?,
                    cleanup_generation_id: row.get(10)?,
                    namespace_ciphertext: row.get(11)?,
                    known_run_set_digest: row.get(12)?,
                    target_digest: row.get(13)?,
                    observation_pass: row.get(14)?,
                    request_epoch: row.get(15)?,
                    fence: row.get(16)?,
                    request_id: row.get(17)?,
                    first_request_started_at_ms: row.get(18)?,
                })
            },
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    validate_workflow_cleanup_managed_cloud_memo(
        stored.managed_cloud_binding_sha256.as_deref(),
        stored.managed_cloud_release_memo_base64url.as_deref(),
        stored.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    let known_rows = {
        let mut statement = tx.prepare(
            "SELECT run_id_hmac_sha256, run_id_ciphertext,
                    run_identity_digest_sha256
               FROM jobs_workflow_cleanup_v2_known_runs
              WHERE account_id = ?1 AND workflow_cleanup_generation = ?2
                AND workflow_id = ?3 ORDER BY run_id_hmac_sha256",
        )?;
        let rows = statement
            .query_map(
                params![stored.account_id, stored.generation, stored.workflow_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let known_run_ids = verify_workflow_cleanup_v2_known_runs(
        &stored.workflow_id,
        &stored.known_run_set_digest,
        known_rows,
    )?;
    if (stored.first_execution_run_id.is_none() != known_run_ids.is_empty())
        || stored
            .first_execution_run_id
            .as_ref()
            .is_some_and(|first| !known_run_ids.contains(first))
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_cleanup_v2_target_authorities
            SET request_epoch = ?1, cleanup_fence = ?2, cleanup_request_id = ?3,
                first_request_started_at_ms = CASE WHEN ?4 = 1
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = ?5,
                lease_token_sha256 = ?6, lease_expires_at_ms = ?7,
                updated_at_ms = MAX(?8, updated_at_ms + 1)
          WHERE account_id = ?9 AND workflow_cleanup_generation = ?10
            AND workflow_id = ?11 AND request_epoch = ?12 AND cleanup_fence = ?13
            AND (lease_owner IS NULL OR lease_expires_at_ms <= ?8)",
        params![
            request_epoch,
            fence,
            cleanup_request_id,
            i64::from(reuse_started_request),
            owner_id,
            lease_hash,
            lease_expires_at_ms,
            now_ms,
            stored.account_id,
            stored.generation,
            stored.workflow_id,
            stored.request_epoch,
            stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow v2 cleanup target changed while being claimed")
    }
    tx.execute(
        "UPDATE jobs_workflow_cleanup_targets
            SET fence = ?1, cleanup_request_id = ?2, lease_owner = ?3,
                lease_token_sha256 = ?4, lease_expires_at_ms = ?5,
                updated_at_ms = MAX(?6, updated_at_ms + 1)
          WHERE account_id = ?7 AND generation = ?8 AND workflow_id = ?9",
        params![
            fence,
            cleanup_request_id,
            owner_id,
            lease_hash,
            lease_expires_at_ms,
            now_ms,
            stored.account_id,
            stored.generation,
            stored.workflow_id,
        ],
    )?;
    Ok(Some(JobsV2TargetCleanupLease {
        cleanup_request_id,
        cleanup_generation_id: stored.cleanup_generation_id,
        target_set_digest: stored.target_set_digest,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_V2_TYPE.to_string(),
        workflow_id: stored.workflow_id,
        first_execution_run_id: stored.first_execution_run_id,
        start_request_id: stored.start_request_id,
        start_payload_digest: stored.start_payload_digest,
        managed_cloud_binding_sha256: stored.managed_cloud_binding_sha256,
        managed_cloud_release_memo_base64url: stored.managed_cloud_release_memo_base64url,
        managed_cloud_release_memo_sha256: stored.managed_cloud_release_memo_sha256,
        known_run_ids,
        target_digest: stored.target_digest,
        cleanup_fence: fence,
        observation_pass: stored.observation_pass,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    }))
}

fn claim_jobs_legacy_target_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsLegacyTargetCleanupLease>> {
    let row = tx.query_opt(
        "SELECT target.generation, legacy.inventory_generation_id,
                legacy.namespace_ciphertext, legacy.visibility_cutoff_ms,
                legacy.query_digest_sha256, target.discovered_scan_pass,
                target.target_identity_hmac_sha256, target.workflow_id_ciphertext,
                target.run_id_ciphertext, target.first_execution_run_id_ciphertext,
                target.target_digest_sha256, target.observation_pass,
                target.proof_epoch, target.request_epoch, target.fence, target.request_id,
                target.first_request_started_at_ms
           FROM jobs_workflow_legacy_targets target
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = target.generation
           JOIN jobs_workflow_legacy_inventory_head head
             ON head.generation = legacy.generation
            AND head.query_digest_sha256 = legacy.query_digest_sha256
          WHERE legacy.state = 'draining'
            AND target.target_state NOT IN ('absence_proved', 'identity_conflict')
            AND NOT target.raw_ids_scrubbed
            AND NOT target.positive_reset_required
            AND target.next_attempt_at_ms <= $1
            AND (target.lease_owner IS NULL OR target.lease_expires_at_ms <= $1)
          ORDER BY target.updated_at_ms, target.generation,
                   target.target_identity_hmac_sha256
          FOR UPDATE OF target SKIP LOCKED LIMIT 1",
        &[&now_ms],
    )?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored = StoredLegacyTargetClaim {
        generation: row.get(0),
        inventory_generation_id: row.get(1),
        namespace_ciphertext: row.get(2),
        visibility_cutoff_ms: row.get(3),
        query_digest: row.get(4),
        scan_pass: row.get(5),
        target_identity_hmac: row.get(6),
        workflow_id_ciphertext: row.get(7),
        run_id_ciphertext: row.get(8),
        first_run_id_ciphertext: row.get(9),
        target_digest: row.get(10),
        observation_pass: row.get(11),
        proof_epoch: row.get(12),
        request_epoch: row.get(13),
        fence: row.get(14),
        request_id: row.get(15),
        first_request_started_at_ms: row.get(16),
    };
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_legacy_targets
            SET request_epoch = $1, fence = $2, request_id = $3,
                first_request_started_at_ms = CASE WHEN $4
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = $5,
                lease_token_sha256 = $6, lease_expires_at_ms = $7,
                updated_at_ms = GREATEST($8, updated_at_ms + 1)
          WHERE generation = $9 AND target_identity_hmac_sha256 = $10
            AND proof_epoch = $11 AND request_epoch = $12 AND fence = $13
            AND (lease_owner IS NULL OR lease_expires_at_ms <= $8)",
        &[
            &request_epoch,
            &fence,
            &cleanup_request_id,
            &reuse_started_request,
            &owner_id,
            &lease_hash,
            &lease_expires_at_ms,
            &now_ms,
            &stored.generation,
            &stored.target_identity_hmac,
            &stored.proof_epoch,
            &stored.request_epoch,
            &stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow legacy cleanup target changed while being claimed")
    }
    let lease = JobsLegacyTargetCleanupLease {
        cleanup_request_id,
        inventory_generation_id: stored.inventory_generation_id,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
        visibility_cutoff_ms: stored.visibility_cutoff_ms,
        query_digest: stored.query_digest,
        scan_pass: stored.scan_pass,
        workflow_id: decrypt_payload(&stored.workflow_id_ciphertext)?,
        run_id: decrypt_payload(&stored.run_id_ciphertext)?,
        first_execution_run_id: decrypt_payload(&stored.first_run_id_ciphertext)?,
        target_digest: stored.target_digest,
        cleanup_fence: fence,
        observation_pass: stored.observation_pass,
        proof_epoch: stored.proof_epoch,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    };
    if !workflow_cleanup_legacy_workflow_id(&lease.workflow_id)
        || !workflow_command_opaque_identifier(&lease.run_id, 128)
        || !workflow_command_opaque_identifier(&lease.first_execution_run_id, 128)
        || workflow_cleanup_legacy_target_digest(&lease)? != lease.target_digest
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(Some(lease))
}

fn claim_jobs_legacy_page_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsLegacyInventoryPageLease>> {
    let row = tx.query_opt(
        "SELECT legacy.generation, legacy.inventory_generation_id,
                legacy.namespace_ciphertext, legacy.visibility_cutoff_ms,
                legacy.query_digest_sha256, legacy.scan_pass, legacy.page_index,
                legacy.predecessor_page_digest_sha256, legacy.page_token_ciphertext,
                legacy.request_epoch, legacy.fence, legacy.request_id,
                legacy.first_request_started_at_ms
           FROM jobs_workflow_legacy_inventory_head head
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = head.generation
            AND legacy.query_digest_sha256 = head.query_digest_sha256
          WHERE head.singleton_id = 1 AND legacy.state = 'scanning'
            AND legacy.next_attempt_at_ms <= $1
            AND (legacy.lease_owner IS NULL OR legacy.lease_expires_at_ms <= $1)
          FOR UPDATE OF legacy SKIP LOCKED",
        &[&now_ms],
    )?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored = StoredLegacyPageClaim {
        generation: row.get(0),
        inventory_generation_id: row.get(1),
        namespace_ciphertext: row.get(2),
        visibility_cutoff_ms: row.get(3),
        query_digest: row.get(4),
        scan_pass: row.get(5),
        page_index: row.get(6),
        predecessor_page_digest: row.get(7),
        page_token_ciphertext: row.get(8),
        request_epoch: row.get(9),
        fence: row.get(10),
        request_id: row.get(11),
        first_request_started_at_ms: row.get(12),
    };
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_legacy_inventory_generations
            SET request_epoch = $1, fence = $2, request_id = $3,
                first_request_started_at_ms = CASE WHEN $4
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = $5,
                lease_token_sha256 = $6, lease_expires_at_ms = $7,
                updated_at_ms = GREATEST($8, updated_at_ms + 1)
          WHERE generation = $9 AND request_epoch = $10 AND fence = $11
            AND state = 'scanning'
            AND (lease_owner IS NULL OR lease_expires_at_ms <= $8)",
        &[
            &request_epoch,
            &fence,
            &cleanup_request_id,
            &reuse_started_request,
            &owner_id,
            &lease_hash,
            &lease_expires_at_ms,
            &now_ms,
            &stored.generation,
            &stored.request_epoch,
            &stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow legacy inventory page changed while being claimed")
    }
    let page_token = stored
        .page_token_ciphertext
        .as_deref()
        .map(decrypt_payload)
        .transpose()?;
    if !workflow_cleanup_page_token(page_token.as_deref()) {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(Some(JobsLegacyInventoryPageLease {
        cleanup_request_id,
        inventory_generation_id: stored.inventory_generation_id,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
        visibility_cutoff_ms: stored.visibility_cutoff_ms,
        query_digest: stored.query_digest,
        scan_pass: stored.scan_pass,
        page_index: stored.page_index,
        predecessor_page_digest: (stored.page_index > 0).then_some(stored.predecessor_page_digest),
        page_token,
        cleanup_fence: fence,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    }))
}

fn claim_jobs_v2_target_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    owner_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> Result<Option<JobsV2TargetCleanupLease>> {
    let row = tx.query_opt(
        "SELECT target.account_id, target.generation, target.workflow_id,
                target.first_execution_run_id, target.start_request_id,
                target.start_payload_hmac_sha256,
                target.managed_cloud_binding_sha256,
                target.managed_cloud_release_memo_base64url,
                target.managed_cloud_release_memo_sha256,
                target.target_set_hmac_sha256,
                binding.cleanup_generation_id, legacy.namespace_ciphertext,
                authority.known_run_set_digest_sha256,
                authority.target_digest_sha256, authority.observation_pass,
                authority.request_epoch, authority.cleanup_fence,
                authority.cleanup_request_id, authority.first_request_started_at_ms
           FROM jobs_workflow_cleanup_v2_target_authorities authority
           JOIN jobs_workflow_cleanup_targets target
             ON target.account_id = authority.account_id
            AND target.generation = authority.workflow_cleanup_generation
            AND target.workflow_id = authority.workflow_id
           JOIN jobs_workflow_cleanup_account_bindings binding
             ON binding.account_id = target.account_id
            AND binding.workflow_cleanup_generation = target.generation
            AND binding.target_set_hmac_sha256 = target.target_set_hmac_sha256
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = binding.legacy_generation
            AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
          WHERE binding.state IN ('frozen', 'draining')
            AND NOT authority.positive_reset_required
            AND target.target_state NOT IN (
              'delivery_drain', 'absence_proved', 'identity_conflict'
            )
            AND authority.next_attempt_at_ms <= $1
            AND (authority.lease_owner IS NULL OR authority.lease_expires_at_ms <= $1)
          ORDER BY authority.updated_at_ms, authority.account_id, authority.workflow_id
          FOR UPDATE OF authority SKIP LOCKED LIMIT 1",
        &[&now_ms],
    )?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored = StoredV2TargetClaim {
        account_id: row.get(0),
        generation: row.get(1),
        workflow_id: row.get(2),
        first_execution_run_id: row.get(3),
        start_request_id: row.get(4),
        start_payload_digest: row.get(5),
        managed_cloud_binding_sha256: row.get(6),
        managed_cloud_release_memo_base64url: row.get(7),
        managed_cloud_release_memo_sha256: row.get(8),
        target_set_digest: row.get(9),
        cleanup_generation_id: row.get(10),
        namespace_ciphertext: row.get(11),
        known_run_set_digest: row.get(12),
        target_digest: row.get(13),
        observation_pass: row.get(14),
        request_epoch: row.get(15),
        fence: row.get(16),
        request_id: row.get(17),
        first_request_started_at_ms: row.get(18),
    };
    validate_workflow_cleanup_managed_cloud_memo(
        stored.managed_cloud_binding_sha256.as_deref(),
        stored.managed_cloud_release_memo_base64url.as_deref(),
        stored.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    // Preserve the explicit authority -> target order used by receipts and
    // hard deletion. Locking both joined relations is planner-dependent.
    tx.query_one(
        "SELECT 1 FROM jobs_workflow_cleanup_targets
          WHERE account_id = $1 AND generation = $2 AND workflow_id = $3
            AND target_set_hmac_sha256 = $4
            AND target_state NOT IN ('delivery_drain', 'absence_proved', 'identity_conflict')
          FOR UPDATE",
        &[
            &stored.account_id,
            &stored.generation,
            &stored.workflow_id,
            &stored.target_set_digest,
        ],
    )?;
    let known_rows = tx.query(
        "SELECT run_id_hmac_sha256, run_id_ciphertext,
                run_identity_digest_sha256
           FROM jobs_workflow_cleanup_v2_known_runs
          WHERE account_id = $1 AND workflow_cleanup_generation = $2
            AND workflow_id = $3 ORDER BY run_id_hmac_sha256 FOR SHARE",
        &[&stored.account_id, &stored.generation, &stored.workflow_id],
    )?;
    let known_run_ids = verify_workflow_cleanup_v2_known_runs(
        &stored.workflow_id,
        &stored.known_run_set_digest,
        known_rows
            .iter()
            .map(|row| (row.get(0), row.get(1), row.get(2)))
            .collect(),
    )?;
    if (stored.first_execution_run_id.is_none() != known_run_ids.is_empty())
        || stored
            .first_execution_run_id
            .as_ref()
            .is_some_and(|first| !known_run_ids.contains(first))
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    let reuse_started_request = stored.first_request_started_at_ms.is_some();
    let (request_epoch, fence, cleanup_request_id) = next_cleanup_request_identity(
        stored.request_epoch,
        stored.fence,
        stored.request_id.clone(),
        stored.first_request_started_at_ms,
    )?;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    if tx.execute(
        "UPDATE jobs_workflow_cleanup_v2_target_authorities
            SET request_epoch = $1, cleanup_fence = $2, cleanup_request_id = $3,
                first_request_started_at_ms = CASE WHEN $4
                  THEN first_request_started_at_ms ELSE NULL END,
                last_outcome_code = NULL, lease_owner = $5,
                lease_token_sha256 = $6, lease_expires_at_ms = $7,
                updated_at_ms = GREATEST($8, updated_at_ms + 1)
          WHERE account_id = $9 AND workflow_cleanup_generation = $10
            AND workflow_id = $11 AND request_epoch = $12 AND cleanup_fence = $13
            AND (lease_owner IS NULL OR lease_expires_at_ms <= $8)",
        &[
            &request_epoch,
            &fence,
            &cleanup_request_id,
            &reuse_started_request,
            &owner_id,
            &lease_hash,
            &lease_expires_at_ms,
            &now_ms,
            &stored.account_id,
            &stored.generation,
            &stored.workflow_id,
            &stored.request_epoch,
            &stored.fence,
        ],
    )? != 1
    {
        anyhow::bail!("workflow v2 cleanup target changed while being claimed")
    }
    tx.execute(
        "UPDATE jobs_workflow_cleanup_targets
            SET fence = $1, cleanup_request_id = $2, lease_owner = $3,
                lease_token_sha256 = $4, lease_expires_at_ms = $5,
                updated_at_ms = GREATEST($6, updated_at_ms + 1)
          WHERE account_id = $7 AND generation = $8 AND workflow_id = $9",
        &[
            &fence,
            &cleanup_request_id,
            &owner_id,
            &lease_hash,
            &lease_expires_at_ms,
            &now_ms,
            &stored.account_id,
            &stored.generation,
            &stored.workflow_id,
        ],
    )?;
    Ok(Some(JobsV2TargetCleanupLease {
        cleanup_request_id,
        cleanup_generation_id: stored.cleanup_generation_id,
        target_set_digest: stored.target_set_digest,
        namespace: decrypt_payload(&stored.namespace_ciphertext)?,
        workflow_type: WORKFLOW_V2_TYPE.to_string(),
        workflow_id: stored.workflow_id,
        first_execution_run_id: stored.first_execution_run_id,
        start_request_id: stored.start_request_id,
        start_payload_digest: stored.start_payload_digest,
        managed_cloud_binding_sha256: stored.managed_cloud_binding_sha256,
        managed_cloud_release_memo_base64url: stored.managed_cloud_release_memo_base64url,
        managed_cloud_release_memo_sha256: stored.managed_cloud_release_memo_sha256,
        known_run_ids,
        target_digest: stored.target_digest,
        cleanup_fence: fence,
        observation_pass: stored.observation_pass,
        request_epoch,
        lease_owner: owner_id.to_string(),
        lease_token,
        lease_expires_at_ms,
    }))
}

pub fn mark_jobs_workflow_cleanup_request_started(
    pool: &DbPool,
    lease: &JobsWorkflowCleanupWorkLease,
    now_ms: i64,
) -> Result<()> {
    let (_, request_epoch, fence, _, token, expires_at_ms) = lease.request_identity();
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
        || now_ms > expires_at_ms
        || request_epoch < 1
        || fence < 1
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if let JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) = lease {
        validate_workflow_cleanup_managed_cloud_memo(
            lease.managed_cloud_binding_sha256.as_deref(),
            lease.managed_cloud_release_memo_base64url.as_deref(),
            lease.managed_cloud_release_memo_sha256.as_deref(),
        )?;
    }
    let lease_hash = workflow_command_lease_token_sha256(token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let connection = pool.get()?;
            let authority_now_ms: i64 = connection.query_row(
                "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
                [],
                |row| row.get(0),
            )?;
            let updated = match lease {
                JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => connection.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET first_request_started_at_ms = COALESCE(
                              first_request_started_at_ms, ?1),
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE inventory_generation_id = ?2 AND query_digest_sha256 = ?3
                        AND request_epoch = ?4 AND fence = ?5 AND request_id = ?6
                        AND lease_owner = ?7 AND lease_token_sha256 = ?8
                        AND lease_expires_at_ms = ?9 AND lease_expires_at_ms >= ?1",
                    params![
                        authority_now_ms,
                        lease.inventory_generation_id,
                        lease.query_digest,
                        lease.request_epoch,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        lease.lease_owner,
                        lease_hash,
                        lease.lease_expires_at_ms,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => {
                    let target_hmac = workflow_command_hmac(
                        "legacy-cleanup-target-index-v3",
                        &json!({
                            "firstExecutionRunId": lease.first_execution_run_id,
                            "runId": lease.run_id,
                            "workflowId": lease.workflow_id,
                        }),
                        2_048,
                    )?;
                    connection.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET first_request_started_at_ms = COALESCE(
                                  first_request_started_at_ms, ?1),
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE target_identity_hmac_sha256 = ?2
                            AND target_digest_sha256 = ?3
                            AND request_epoch = ?4 AND fence = ?5 AND request_id = ?6
                            AND lease_owner = ?7 AND lease_token_sha256 = ?8
                            AND lease_expires_at_ms = ?9 AND lease_expires_at_ms >= ?1
                            AND proof_epoch = ?10",
                        params![
                            authority_now_ms,
                            target_hmac,
                            lease.target_digest,
                            lease.request_epoch,
                            lease.cleanup_fence,
                            lease.cleanup_request_id,
                            lease.lease_owner,
                            lease_hash,
                            lease.lease_expires_at_ms,
                            lease.proof_epoch,
                        ],
                    )?
                }
                JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => connection.execute(
                    "UPDATE jobs_workflow_cleanup_v2_target_authorities
                        SET first_request_started_at_ms = COALESCE(
                              first_request_started_at_ms, ?1),
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE workflow_id = ?2 AND target_digest_sha256 = ?3
                        AND request_epoch = ?4 AND cleanup_fence = ?5
                        AND cleanup_request_id = ?6 AND lease_owner = ?7
                        AND lease_token_sha256 = ?8 AND lease_expires_at_ms = ?9
                        AND lease_expires_at_ms >= ?1
                        AND EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_targets target
                           WHERE target.account_id =
                                 jobs_workflow_cleanup_v2_target_authorities.account_id
                             AND target.generation =
                                 jobs_workflow_cleanup_v2_target_authorities.workflow_cleanup_generation
                             AND target.workflow_id =
                                 jobs_workflow_cleanup_v2_target_authorities.workflow_id
                             AND target.managed_cloud_binding_sha256 IS ?10
                             AND target.managed_cloud_release_memo_base64url IS ?11
                             AND target.managed_cloud_release_memo_sha256 IS ?12
                        )",
                    params![
                        authority_now_ms,
                        lease.workflow_id,
                        lease.target_digest,
                        lease.request_epoch,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        lease.lease_owner,
                        lease_hash,
                        lease.lease_expires_at_ms,
                        lease.managed_cloud_binding_sha256,
                        lease.managed_cloud_release_memo_base64url,
                        lease.managed_cloud_release_memo_sha256,
                    ],
                )?,
            };
            if updated != 1 {
                anyhow::bail!("workflow cleanup request-start lease is not current")
            }
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let authority_now_ms: i64 = connection
                .query_one(
                    "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )?
                .get(0);
            let updated = match lease {
                JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => connection.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET first_request_started_at_ms = COALESCE(
                              first_request_started_at_ms, $1),
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE inventory_generation_id = $2 AND query_digest_sha256 = $3
                        AND request_epoch = $4 AND fence = $5 AND request_id = $6
                        AND lease_owner = $7 AND lease_token_sha256 = $8
                        AND lease_expires_at_ms = $9 AND lease_expires_at_ms >= $1",
                    &[
                        &authority_now_ms,
                        &lease.inventory_generation_id,
                        &lease.query_digest,
                        &lease.request_epoch,
                        &lease.cleanup_fence,
                        &lease.cleanup_request_id,
                        &lease.lease_owner,
                        &lease_hash,
                        &lease.lease_expires_at_ms,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => {
                    let target_hmac = workflow_command_hmac(
                        "legacy-cleanup-target-index-v3",
                        &json!({
                            "firstExecutionRunId": lease.first_execution_run_id,
                            "runId": lease.run_id,
                            "workflowId": lease.workflow_id,
                        }),
                        2_048,
                    )?;
                    connection.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET first_request_started_at_ms = COALESCE(
                                  first_request_started_at_ms, $1),
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE target_identity_hmac_sha256 = $2
                            AND target_digest_sha256 = $3
                            AND request_epoch = $4 AND fence = $5 AND request_id = $6
                            AND lease_owner = $7 AND lease_token_sha256 = $8
                            AND lease_expires_at_ms = $9 AND lease_expires_at_ms >= $1
                            AND proof_epoch = $10",
                        &[
                            &authority_now_ms,
                            &target_hmac,
                            &lease.target_digest,
                            &lease.request_epoch,
                            &lease.cleanup_fence,
                            &lease.cleanup_request_id,
                            &lease.lease_owner,
                            &lease_hash,
                            &lease.lease_expires_at_ms,
                            &lease.proof_epoch,
                        ],
                    )?
                }
                JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => connection.execute(
                    "UPDATE jobs_workflow_cleanup_v2_target_authorities
                        SET first_request_started_at_ms = COALESCE(
                              first_request_started_at_ms, $1),
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE workflow_id = $2 AND target_digest_sha256 = $3
                        AND request_epoch = $4 AND cleanup_fence = $5
                        AND cleanup_request_id = $6 AND lease_owner = $7
                        AND lease_token_sha256 = $8 AND lease_expires_at_ms = $9
                        AND lease_expires_at_ms >= $1
                        AND EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_targets target
                           WHERE target.account_id =
                                 jobs_workflow_cleanup_v2_target_authorities.account_id
                             AND target.generation =
                                 jobs_workflow_cleanup_v2_target_authorities.workflow_cleanup_generation
                             AND target.workflow_id =
                                 jobs_workflow_cleanup_v2_target_authorities.workflow_id
                             AND target.managed_cloud_binding_sha256
                                 IS NOT DISTINCT FROM $10
                             AND target.managed_cloud_release_memo_base64url
                                 IS NOT DISTINCT FROM $11
                             AND target.managed_cloud_release_memo_sha256
                                 IS NOT DISTINCT FROM $12
                        )",
                    &[
                        &authority_now_ms,
                        &lease.workflow_id,
                        &lease.target_digest,
                        &lease.request_epoch,
                        &lease.cleanup_fence,
                        &lease.cleanup_request_id,
                        &lease.lease_owner,
                        &lease_hash,
                        &lease.lease_expires_at_ms,
                        &lease.managed_cloud_binding_sha256,
                        &lease.managed_cloud_release_memo_base64url,
                        &lease.managed_cloud_release_memo_sha256,
                    ],
                )?,
            };
            if updated != 1 {
                anyhow::bail!("workflow cleanup request-start lease is not current")
            }
            Ok(())
        }
    })
}

pub fn record_jobs_workflow_cleanup_delivery_failure(
    pool: &DbPool,
    lease: &JobsWorkflowCleanupWorkLease,
    failure: JobsWorkflowCleanupDeliveryFailure,
    now_ms: i64,
) -> Result<()> {
    let (_, request_epoch, fence, _, token, expires_at_ms) = lease.request_identity();
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
        || now_ms > expires_at_ms
        || request_epoch < 1
        || fence < 1
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let outcome = match failure {
        JobsWorkflowCleanupDeliveryFailure::TransportUnknown => "transport_unknown",
        JobsWorkflowCleanupDeliveryFailure::GatewayUnavailable => "gateway_unavailable",
        JobsWorkflowCleanupDeliveryFailure::IdentityConflict => "identity_conflict",
    };
    let lease_hash = workflow_command_lease_token_sha256(token);
    let identity_conflict_evidence = match (lease, failure) {
        (
            JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease),
            JobsWorkflowCleanupDeliveryFailure::IdentityConflict,
        ) => Some(workflow_command_hmac(
            "workflow-cleanup-delivery-failure-evidence-v3",
            &json!({
                "cleanupFence": lease.cleanup_fence,
                "cleanupGenerationId": lease.cleanup_generation_id,
                "cleanupRequestId": lease.cleanup_request_id,
                "failure": "identity_conflict",
                "requestEpoch": lease.request_epoch,
                "targetDigest": lease.target_digest,
                "targetSetDigest": lease.target_set_digest,
                "workflowId": lease.workflow_id,
            }),
            4_096,
        )?),
        _ => None,
    };
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let next_attempt_at_ms = now_ms
                .checked_add(WORKFLOW_CLEANUP_RETRY_MS)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            let updated = match lease {
                JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = CASE WHEN ?1 = 'identity_conflict'
                              THEN 'identity_conflict' ELSE state END,
                            last_outcome_code = ?1, lease_expires_at_ms = ?2,
                            next_attempt_at_ms = ?3,
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE inventory_generation_id = ?4 AND query_digest_sha256 = ?5
                        AND request_epoch = ?6 AND fence = ?7 AND request_id = ?8
                        AND first_request_started_at_ms IS NOT NULL
                        AND lease_owner = ?9 AND lease_token_sha256 = ?10
                        AND lease_expires_at_ms = ?11 AND lease_expires_at_ms >= ?2",
                    params![
                        outcome,
                        now_ms,
                        next_attempt_at_ms,
                        lease.inventory_generation_id,
                        lease.query_digest,
                        lease.request_epoch,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        lease.lease_owner,
                        lease_hash,
                        lease.lease_expires_at_ms,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = CASE WHEN ?1 = 'identity_conflict'
                              THEN 'identity_conflict' ELSE target_state END,
                            last_outcome_code = ?1, lease_expires_at_ms = ?2,
                            next_attempt_at_ms = ?3,
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE target_digest_sha256 = ?4 AND request_epoch = ?5
                        AND fence = ?6 AND request_id = ?7
                        AND first_request_started_at_ms IS NOT NULL
                        AND lease_owner = ?8 AND lease_token_sha256 = ?9
                        AND lease_expires_at_ms = ?10 AND lease_expires_at_ms >= ?2
                        AND proof_epoch = ?11",
                    params![
                        outcome,
                        now_ms,
                        next_attempt_at_ms,
                        lease.target_digest,
                        lease.request_epoch,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        lease.lease_owner,
                        lease_hash,
                        lease.lease_expires_at_ms,
                        lease.proof_epoch,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => {
                    if let Some(evidence) = identity_conflict_evidence.as_deref() {
                        if let Some(stored) = tx
                            .query_row(
                                "SELECT evidence_hmac_sha256
                                   FROM jobs_workflow_execution_cleanup_observations
                                  WHERE workflow_id = ?1
                                    AND target_set_hmac_sha256 = ?2
                                    AND cleanup_fence = ?3
                                    AND cleanup_request_id = ?4
                                    AND observation_kind = 'identity_conflict'",
                                params![
                                    lease.workflow_id,
                                    lease.target_set_digest,
                                    lease.cleanup_fence,
                                    lease.cleanup_request_id,
                                ],
                                |row| row.get::<_, String>(0),
                            )
                            .optional()?
                        {
                            if stored != evidence {
                                return Err(JobsWorkflowCommandError::IdentityConflict.into());
                            }
                            tx.commit()?;
                            return Ok(());
                        }
                    }
                    let updated = tx.execute(
                        "UPDATE jobs_workflow_cleanup_v2_target_authorities
                            SET last_outcome_code = ?1, lease_expires_at_ms = ?2,
                                next_attempt_at_ms = ?3,
                                updated_at_ms = MAX(?2, updated_at_ms + 1)
                          WHERE target_digest_sha256 = ?4 AND request_epoch = ?5
                            AND cleanup_fence = ?6 AND cleanup_request_id = ?7
                            AND first_request_started_at_ms IS NOT NULL
                            AND lease_owner = ?8 AND lease_token_sha256 = ?9
                            AND lease_expires_at_ms = ?10 AND lease_expires_at_ms >= ?2",
                        params![
                            outcome,
                            now_ms,
                            next_attempt_at_ms,
                            lease.target_digest,
                            lease.request_epoch,
                            lease.cleanup_fence,
                            lease.cleanup_request_id,
                            lease.lease_owner,
                            lease_hash,
                            lease.lease_expires_at_ms,
                        ],
                    )?;
                    if let Some(evidence) = identity_conflict_evidence
                        .as_deref()
                        .filter(|_| updated == 1)
                    {
                        let observation_id = format!("wfv2conflict-v3-{}", &evidence[..32]);
                        if tx.execute(
                            "INSERT INTO jobs_workflow_execution_cleanup_observations (
                                    id, account_id, workflow_id, generation,
                                    target_set_hmac_sha256, observation_kind,
                                    observed_execution_run_id, cleanup_fence,
                                    cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
                                 )
                                 SELECT ?1, target.account_id, target.workflow_id,
                                        target.generation, target.target_set_hmac_sha256,
                                        'identity_conflict', target.first_execution_run_id,
                                        target.fence, target.cleanup_request_id, ?2, ?3
                                   FROM jobs_workflow_cleanup_v2_target_authorities authority
                                   JOIN jobs_workflow_cleanup_targets target
                                     ON target.account_id = authority.account_id
                                    AND target.generation = authority.workflow_cleanup_generation
                                    AND target.workflow_id = authority.workflow_id
                                  WHERE authority.workflow_id = ?4
                                    AND authority.target_digest_sha256 = ?5
                                    AND target.target_set_hmac_sha256 = ?6
                                    AND target.fence = ?7
                                    AND target.cleanup_request_id = ?8",
                            params![
                                observation_id,
                                evidence,
                                now_ms,
                                lease.workflow_id,
                                lease.target_digest,
                                lease.target_set_digest,
                                lease.cleanup_fence,
                                lease.cleanup_request_id,
                            ],
                        )? != 1
                        {
                            anyhow::bail!(
                                "workflow cleanup identity-conflict evidence target is not current"
                            )
                        }
                    }
                    if updated == 1
                        && failure == JobsWorkflowCleanupDeliveryFailure::IdentityConflict
                        && tx.execute(
                            "UPDATE jobs_workflow_cleanup_targets
                                SET target_state = 'identity_conflict',
                                    updated_at_ms = MAX(?1, updated_at_ms + 1)
                              WHERE workflow_id = ?2 AND target_set_hmac_sha256 = ?3
                                AND fence = ?4 AND cleanup_request_id = ?5",
                            params![
                                now_ms,
                                lease.workflow_id,
                                lease.target_set_digest,
                                lease.cleanup_fence,
                                lease.cleanup_request_id,
                            ],
                        )? != 1
                    {
                        anyhow::bail!("workflow cleanup identity-conflict target is not current")
                    }
                    updated
                }
            };
            if updated != 1 {
                anyhow::bail!("workflow cleanup failure lease is not current")
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            let next_attempt_at_ms = now_ms
                .checked_add(WORKFLOW_CLEANUP_RETRY_MS)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            let updated = match lease {
                JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = CASE WHEN $1 = 'identity_conflict'
                              THEN 'identity_conflict' ELSE state END,
                            last_outcome_code = $1, lease_expires_at_ms = $2,
                            next_attempt_at_ms = $3,
                            updated_at_ms = GREATEST($2, updated_at_ms + 1)
                      WHERE inventory_generation_id = $4 AND query_digest_sha256 = $5
                        AND request_epoch = $6 AND fence = $7 AND request_id = $8
                        AND first_request_started_at_ms IS NOT NULL
                        AND lease_owner = $9 AND lease_token_sha256 = $10
                        AND lease_expires_at_ms = $11 AND lease_expires_at_ms >= $2",
                    &[
                        &outcome,
                        &now_ms,
                        &next_attempt_at_ms,
                        &lease.inventory_generation_id,
                        &lease.query_digest,
                        &lease.request_epoch,
                        &lease.cleanup_fence,
                        &lease.cleanup_request_id,
                        &lease.lease_owner,
                        &lease_hash,
                        &lease.lease_expires_at_ms,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = CASE WHEN $1 = 'identity_conflict'
                              THEN 'identity_conflict' ELSE target_state END,
                            last_outcome_code = $1, lease_expires_at_ms = $2,
                            next_attempt_at_ms = $3,
                            updated_at_ms = GREATEST($2, updated_at_ms + 1)
                      WHERE target_digest_sha256 = $4 AND request_epoch = $5
                        AND fence = $6 AND request_id = $7
                        AND first_request_started_at_ms IS NOT NULL
                        AND lease_owner = $8 AND lease_token_sha256 = $9
                        AND lease_expires_at_ms = $10 AND lease_expires_at_ms >= $2
                        AND proof_epoch = $11",
                    &[
                        &outcome,
                        &now_ms,
                        &next_attempt_at_ms,
                        &lease.target_digest,
                        &lease.request_epoch,
                        &lease.cleanup_fence,
                        &lease.cleanup_request_id,
                        &lease.lease_owner,
                        &lease_hash,
                        &lease.lease_expires_at_ms,
                        &lease.proof_epoch,
                    ],
                )?,
                JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => {
                    if let Some(evidence) = identity_conflict_evidence.as_deref() {
                        if let Some(row) = tx.query_opt(
                            "SELECT evidence_hmac_sha256
                               FROM jobs_workflow_execution_cleanup_observations
                              WHERE workflow_id = $1
                                AND target_set_hmac_sha256 = $2
                                AND cleanup_fence = $3
                                AND cleanup_request_id = $4
                                AND observation_kind = 'identity_conflict'
                              FOR SHARE",
                            &[
                                &lease.workflow_id,
                                &lease.target_set_digest,
                                &lease.cleanup_fence,
                                &lease.cleanup_request_id,
                            ],
                        )? {
                            if row.get::<_, String>(0) != evidence {
                                return Err(JobsWorkflowCommandError::IdentityConflict.into());
                            }
                            tx.commit()?;
                            return Ok(());
                        }
                    }
                    let updated = tx.execute(
                        "UPDATE jobs_workflow_cleanup_v2_target_authorities
                            SET last_outcome_code = $1, lease_expires_at_ms = $2,
                                next_attempt_at_ms = $3,
                                updated_at_ms = GREATEST($2, updated_at_ms + 1)
                          WHERE target_digest_sha256 = $4 AND request_epoch = $5
                            AND cleanup_fence = $6 AND cleanup_request_id = $7
                            AND first_request_started_at_ms IS NOT NULL
                            AND lease_owner = $8 AND lease_token_sha256 = $9
                            AND lease_expires_at_ms = $10 AND lease_expires_at_ms >= $2",
                        &[
                            &outcome,
                            &now_ms,
                            &next_attempt_at_ms,
                            &lease.target_digest,
                            &lease.request_epoch,
                            &lease.cleanup_fence,
                            &lease.cleanup_request_id,
                            &lease.lease_owner,
                            &lease_hash,
                            &lease.lease_expires_at_ms,
                        ],
                    )?;
                    if let Some(evidence) = identity_conflict_evidence
                        .as_deref()
                        .filter(|_| updated == 1)
                    {
                        let observation_id = format!("wfv2conflict-v3-{}", &evidence[..32]);
                        if tx.execute(
                            "INSERT INTO jobs_workflow_execution_cleanup_observations (
                                    id, account_id, workflow_id, generation,
                                    target_set_hmac_sha256, observation_kind,
                                    observed_execution_run_id, cleanup_fence,
                                    cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
                                 )
                                 SELECT $1, target.account_id, target.workflow_id,
                                        target.generation, target.target_set_hmac_sha256,
                                        'identity_conflict', target.first_execution_run_id,
                                        target.fence, target.cleanup_request_id, $2, $3
                                   FROM jobs_workflow_cleanup_v2_target_authorities authority
                                   JOIN jobs_workflow_cleanup_targets target
                                     ON target.account_id = authority.account_id
                                    AND target.generation = authority.workflow_cleanup_generation
                                    AND target.workflow_id = authority.workflow_id
                                  WHERE authority.workflow_id = $4
                                    AND authority.target_digest_sha256 = $5
                                    AND target.target_set_hmac_sha256 = $6
                                    AND target.fence = $7
                                    AND target.cleanup_request_id = $8",
                            &[
                                &observation_id,
                                &evidence,
                                &now_ms,
                                &lease.workflow_id,
                                &lease.target_digest,
                                &lease.target_set_digest,
                                &lease.cleanup_fence,
                                &lease.cleanup_request_id,
                            ],
                        )? != 1
                        {
                            anyhow::bail!(
                                "workflow cleanup identity-conflict evidence target is not current"
                            )
                        }
                    }
                    if updated == 1
                        && failure == JobsWorkflowCleanupDeliveryFailure::IdentityConflict
                        && tx.execute(
                            "UPDATE jobs_workflow_cleanup_targets
                                SET target_state = 'identity_conflict',
                                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
                              WHERE workflow_id = $2 AND target_set_hmac_sha256 = $3
                                AND fence = $4 AND cleanup_request_id = $5",
                            &[
                                &now_ms,
                                &lease.workflow_id,
                                &lease.target_set_digest,
                                &lease.cleanup_fence,
                                &lease.cleanup_request_id,
                            ],
                        )? != 1
                    {
                        anyhow::bail!("workflow cleanup identity-conflict target is not current")
                    }
                    updated
                }
            };
            if updated != 1 {
                anyhow::bail!("workflow cleanup failure lease is not current")
            }
            tx.commit()?;
            Ok(())
        }
    })
}

fn workflow_cleanup_account_generation(account_id: &str, requested_at_ms: i64) -> Result<i64> {
    let digest = workflow_command_hmac(
        "account-cleanup-generation-v3",
        &json!({
            "accountId": account_id,
            "requestedAtMs": requested_at_ms,
        }),
        1_024,
    )?;
    let prefix = u64::from_be_bytes(
        hex::decode(digest)?[..8]
            .try_into()
            .map_err(|_| JobsWorkflowCommandError::InvalidState)?,
    );
    Ok((prefix % WORKFLOW_COMMAND_SAFE_INTEGER_MAX as u64) as i64 + 1)
}

fn validate_account_cleanup_freeze(
    account_id: &str,
    requested_at_ms: i64,
    legacy: &JobsLegacyInventoryAuthorityRef,
    now_ms: i64,
) -> Result<()> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !workflow_command_opaque_identifier(&legacy.inventory_generation_id, 128)
        || !workflow_cleanup_digest(&legacy.query_digest)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok(())
}

fn workflow_cleanup_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn workflow_cleanup_generation_id(
    account_id: &str,
    requested_at_ms: i64,
    generation: i64,
    target_set_digest: &str,
) -> Result<String> {
    let digest = workflow_command_hmac(
        "account-cleanup-generation-id-v3",
        &json!({
            "accountId": account_id,
            "accountGeneration": requested_at_ms,
            "storageGeneration": generation,
            "targetSetDigest": target_set_digest,
        }),
        1_024,
    )?;
    Ok(format!("wfcleanupgen-v3-{}", &digest[..32]))
}

fn workflow_cleanup_v2_known_run_set_digest(
    workflow_id: &str,
    known_run_ids: &[String],
) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
        &json!({
            "knownRunIds": known_run_ids,
            "workflowId": workflow_id,
        }),
        "workflow v2 known run set",
    )
}

fn verify_workflow_cleanup_v2_known_runs(
    workflow_id: &str,
    expected_set_digest: &str,
    rows: Vec<(String, String, String)>,
) -> Result<Vec<String>> {
    let mut run_ids = Vec::with_capacity(rows.len());
    for (stored_hmac, ciphertext, stored_identity_digest) in rows {
        let run_id = decrypt_payload(&ciphertext)?;
        if !workflow_command_opaque_identifier(&run_id, 128) {
            return Err(JobsWorkflowCommandError::InvalidState.into());
        }
        let expected_hmac = workflow_command_hmac(
            "v2-cleanup-known-run-index-v3",
            &json!({"runId": run_id, "workflowId": workflow_id}),
            1_024,
        )?;
        let expected_identity_digest = workflow_cleanup_sha256(
            WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
            &json!({"runId": run_id, "workflowId": workflow_id}),
            "workflow v2 known run identity",
        )?;
        if !workflow_command_hmac_matches(&stored_hmac, &expected_hmac)
            || !workflow_command_hmac_matches(&stored_identity_digest, &expected_identity_digest)
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        run_ids.push(run_id);
    }
    run_ids.sort();
    if !workflow_cleanup_sorted_unique_identifiers(&run_ids, WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS)
        || !workflow_command_hmac_matches(
            expected_set_digest,
            &workflow_cleanup_v2_known_run_set_digest(workflow_id, &run_ids)?,
        )
    {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(run_ids)
}

fn workflow_cleanup_v2_target_digest(
    cleanup_generation_id: &str,
    target_set_digest: &str,
    namespace: &str,
    target: &FrozenWorkflowCleanupTarget,
) -> Result<String> {
    validate_workflow_cleanup_managed_cloud_memo(
        target.managed_cloud_binding_sha256.as_deref(),
        target.managed_cloud_release_memo_base64url.as_deref(),
        target.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    let mut authority = json!({
        "cleanupGenerationId": cleanup_generation_id,
        "firstExecutionRunId": target.first_execution_run_id,
        "namespace": namespace,
        "startPayloadDigest": target.payload_hmac_sha256,
        "startRequestId": target.request_id,
        "targetSetDigest": target_set_digest,
        "workflowId": target.workflow_id,
        "workflowType": WORKFLOW_V2_TYPE,
    });
    add_workflow_cleanup_managed_cloud_digest_fields(
        &mut authority,
        target.managed_cloud_binding_sha256.as_deref(),
        target.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_V2_TARGET_DIGEST_DOMAIN,
        &authority,
        "workflow v2 cleanup target",
    )
}

fn workflow_cleanup_v2_target_digest_from_lease(
    lease: &JobsV2TargetCleanupLease,
    first_execution_run_id: Option<&str>,
) -> Result<String> {
    validate_workflow_cleanup_managed_cloud_memo(
        lease.managed_cloud_binding_sha256.as_deref(),
        lease.managed_cloud_release_memo_base64url.as_deref(),
        lease.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    let mut authority = json!({
        "cleanupGenerationId": lease.cleanup_generation_id,
        "firstExecutionRunId": first_execution_run_id,
        "namespace": lease.namespace,
        "startPayloadDigest": lease.start_payload_digest,
        "startRequestId": lease.start_request_id,
        "targetSetDigest": lease.target_set_digest,
        "workflowId": lease.workflow_id,
        "workflowType": WORKFLOW_V2_TYPE,
    });
    add_workflow_cleanup_managed_cloud_digest_fields(
        &mut authority,
        lease.managed_cloud_binding_sha256.as_deref(),
        lease.managed_cloud_release_memo_sha256.as_deref(),
    )?;
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_V2_TARGET_DIGEST_DOMAIN,
        &authority,
        "workflow v2 cleanup target",
    )
}

fn add_workflow_cleanup_managed_cloud_digest_fields(
    authority: &mut Value,
    binding_sha256: Option<&str>,
    memo_sha256: Option<&str>,
) -> Result<()> {
    match (binding_sha256, memo_sha256) {
        (None, None) => Ok(()),
        (Some(binding_sha256), Some(memo_sha256))
            if workflow_cleanup_digest(binding_sha256) && workflow_cleanup_digest(memo_sha256) =>
        {
            let object = authority
                .as_object_mut()
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            object.insert(
                "managedCloudBindingSha256".to_string(),
                Value::String(binding_sha256.to_string()),
            );
            object.insert(
                "managedCloudReleaseMemoSha256".to_string(),
                Value::String(memo_sha256.to_string()),
            );
            Ok(())
        }
        _ => Err(JobsWorkflowCommandError::InvalidState.into()),
    }
}

fn validate_jobs_legacy_inventory_page_receipt(
    lease: &JobsLegacyInventoryPageLease,
    value: &Value,
) -> Result<JobsLegacyInventoryPageReceiptV3> {
    let object = value
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    for required in [
        "predecessorPageDigest",
        "pageToken",
        "nextPageToken",
        "pageDigest",
        "targetsDigest",
        "targets",
    ] {
        if !object.contains_key(required) {
            return Err(JobsWorkflowCommandError::InvalidRequest.into());
        }
    }
    let receipt: JobsLegacyInventoryPageReceiptV3 = serde_json::from_value(value.clone())?;
    if receipt.schema_version != WORKFLOW_CLEANUP_PROTOCOL_VERSION
        || receipt.operation != "legacy_inventory_page"
        || receipt.cleanup_request_id != lease.cleanup_request_id
        || receipt.inventory_generation_id != lease.inventory_generation_id
        || receipt.namespace != lease.namespace
        || receipt.workflow_type != WORKFLOW_LEGACY_TYPE
        || receipt.visibility_cutoff_ms != lease.visibility_cutoff_ms
        || receipt.query_digest != lease.query_digest
        || receipt.scan_pass != lease.scan_pass
        || receipt.page_index != lease.page_index
        || !(0..=WORKFLOW_LEGACY_MAX_PAGE_INDEX).contains(&receipt.page_index)
        || receipt.predecessor_page_digest != lease.predecessor_page_digest
        || receipt.page_token != lease.page_token
        || receipt.cleanup_fence != lease.cleanup_fence
        || receipt.outcome != "page"
        || receipt.exhausted != receipt.next_page_token.is_none()
        || (!receipt.exhausted && receipt.page_index == WORKFLOW_LEGACY_MAX_PAGE_INDEX)
        || receipt.targets.len() > WORKFLOW_LEGACY_PAGE_SIZE as usize
        || !workflow_cleanup_page_token(receipt.next_page_token.as_deref())
        || !workflow_cleanup_digest(&receipt.targets_digest)
        || !workflow_cleanup_digest(&receipt.page_digest)
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let mut previous: Option<(&str, &str)> = None;
    for target in &receipt.targets {
        let identity = (target.workflow_id.as_str(), target.run_id.as_str());
        if !workflow_cleanup_legacy_workflow_id(&target.workflow_id)
            || !workflow_command_opaque_identifier(&target.run_id, 128)
            || !workflow_command_opaque_identifier(&target.first_execution_run_id, 128)
            || !matches!(
                target.status.as_str(),
                "RUNNING"
                    | "COMPLETED"
                    | "FAILED"
                    | "CANCELED"
                    | "TERMINATED"
                    | "CONTINUED_AS_NEW"
                    | "TIMED_OUT"
            )
            || previous.is_some_and(|stored| stored >= identity)
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        previous = Some(identity);
    }
    let targets_value = serde_json::to_value(&receipt.targets)?;
    if workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_TARGETS_DIGEST_DOMAIN,
        &targets_value,
        "workflow legacy inventory targets",
    )? != receipt.targets_digest
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let mut page_value = value.clone();
    page_value
        .as_object_mut()
        .and_then(|page| page.remove("pageDigest"))
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_PAGE_DIGEST_DOMAIN,
        &page_value,
        "workflow legacy inventory page",
    )? != receipt.page_digest
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(receipt)
}

fn validate_jobs_legacy_target_receipt(
    lease: &JobsLegacyTargetCleanupLease,
    value: &Value,
) -> Result<JobsLegacyTargetReceiptV3> {
    let receipt: JobsLegacyTargetReceiptV3 = serde_json::from_value(value.clone())?;
    let pending_reason = matches!(
        receipt.reason.as_str(),
        "workflow_running"
            | "history_delete_pending"
            | "visibility_pending"
            | "temporal_unavailable"
    );
    if receipt.schema_version != WORKFLOW_CLEANUP_PROTOCOL_VERSION
        || receipt.operation != "reconcile_legacy_target"
        || receipt.cleanup_request_id != lease.cleanup_request_id
        || receipt.inventory_generation_id != lease.inventory_generation_id
        || receipt.namespace != lease.namespace
        || receipt.workflow_type != WORKFLOW_LEGACY_TYPE
        || receipt.visibility_cutoff_ms != lease.visibility_cutoff_ms
        || receipt.query_digest != lease.query_digest
        || receipt.scan_pass != lease.scan_pass
        || receipt.workflow_id != lease.workflow_id
        || receipt.run_id != lease.run_id
        || receipt.first_execution_run_id != lease.first_execution_run_id
        || receipt.target_digest != lease.target_digest
        || receipt.cleanup_fence != lease.cleanup_fence
        || receipt.observation_pass != lease.observation_pass
        || receipt.run_ids != [lease.run_id.clone()]
        || !((receipt.outcome == "pending" && pending_reason)
            || (receipt.outcome == "absence_observed" && receipt.reason == "absence_observed"))
        || !workflow_cleanup_digest(&receipt.evidence_digest)
        || workflow_cleanup_evidence_digest(value)? != receipt.evidence_digest
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(receipt)
}

fn validate_jobs_v2_target_receipt(
    lease: &JobsV2TargetCleanupLease,
    value: &Value,
) -> Result<JobsV2TargetReceiptV3> {
    let object = value
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if !object.contains_key("firstExecutionRunId") {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let receipt: JobsV2TargetReceiptV3 = serde_json::from_value(value.clone())?;
    let pending_reason = matches!(
        receipt.reason.as_str(),
        "termination_pending"
            | "history_delete_pending"
            | "visibility_pending"
            | "temporal_unavailable"
    );
    let new_identity = receipt.run_ids != lease.known_run_ids
        || receipt.first_execution_run_id != lease.first_execution_run_id;
    if receipt.schema_version != WORKFLOW_CLEANUP_PROTOCOL_VERSION
        || receipt.operation != "reconcile_v2_target"
        || receipt.cleanup_request_id != lease.cleanup_request_id
        || receipt.cleanup_generation_id != lease.cleanup_generation_id
        || receipt.target_set_digest != lease.target_set_digest
        || receipt.namespace != lease.namespace
        || receipt.workflow_type != WORKFLOW_V2_TYPE
        || receipt.workflow_id != lease.workflow_id
        || receipt.start_request_id != lease.start_request_id
        || receipt.start_payload_digest != lease.start_payload_digest
        || receipt.managed_cloud_binding_sha256 != lease.managed_cloud_binding_sha256
        || receipt.managed_cloud_release_memo_base64url
            != lease.managed_cloud_release_memo_base64url
        || receipt.managed_cloud_release_memo_sha256 != lease.managed_cloud_release_memo_sha256
        || receipt.known_run_ids != lease.known_run_ids
        || receipt.target_digest != lease.target_digest
        || receipt.cleanup_fence != lease.cleanup_fence
        || receipt.observation_pass != lease.observation_pass
        || !workflow_cleanup_sorted_unique_identifiers(
            &receipt.run_ids,
            WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS,
        )
        || !lease
            .known_run_ids
            .iter()
            .all(|run_id| receipt.run_ids.binary_search(run_id).is_ok())
        || (receipt.first_execution_run_id.is_none() != receipt.run_ids.is_empty())
        || receipt
            .first_execution_run_id
            .as_ref()
            .is_some_and(|first| receipt.run_ids.binary_search(first).is_err())
        || !workflow_cleanup_v2_first_run_transition_allowed(
            lease.first_execution_run_id.as_deref(),
            receipt.first_execution_run_id.as_deref(),
        )
        || !((receipt.outcome == "pending" && pending_reason)
            || (receipt.outcome == "absence_observed" && receipt.reason == "absence_observed"))
        || (new_identity
            && !(receipt.outcome == "pending" && receipt.reason == "visibility_pending"))
        || !workflow_cleanup_digest(&receipt.evidence_digest)
        || workflow_cleanup_evidence_digest(value)? != receipt.evidence_digest
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(receipt)
}

fn workflow_cleanup_v2_first_run_transition_allowed(
    stored: Option<&str>,
    observed: Option<&str>,
) -> bool {
    match stored {
        Some(stored) => observed == Some(stored),
        None => true,
    }
}

fn workflow_cleanup_page_token_hmac(token: &str) -> Result<String> {
    workflow_command_hmac(
        "legacy-inventory-page-token-index-v3",
        &json!({"pageToken": token}),
        WORKFLOW_LEGACY_TOKEN_MAX_BYTES * 2,
    )
}

fn workflow_cleanup_legacy_target_hmac(
    workflow_id: &str,
    run_id: &str,
    first_execution_run_id: &str,
) -> Result<String> {
    workflow_command_hmac(
        "legacy-cleanup-target-index-v3",
        &json!({
            "firstExecutionRunId": first_execution_run_id,
            "runId": run_id,
            "workflowId": workflow_id,
        }),
        2_048,
    )
}

pub fn record_jobs_workflow_cleanup_receipt(
    pool: &DbPool,
    lease: &JobsWorkflowCleanupWorkLease,
    receipt: &Value,
    now_ms: i64,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let (_, request_epoch, fence, _, _, expires_at_ms) = lease.request_identity();
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
        || request_epoch < 1
        || fence < 1
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&expires_at_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    match lease {
        JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => {
            let parsed = validate_jobs_legacy_inventory_page_receipt(lease, receipt)?;
            record_jobs_legacy_inventory_page_receipt(pool, lease, &parsed, now_ms)
        }
        JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => {
            let parsed = validate_jobs_legacy_target_receipt(lease, receipt)?;
            record_jobs_legacy_target_receipt(pool, lease, &parsed, now_ms)
        }
        JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => {
            let parsed = validate_jobs_v2_target_receipt(lease, receipt)?;
            record_jobs_v2_target_receipt(pool, lease, &parsed, now_ms)
        }
    }
}

fn record_jobs_legacy_inventory_page_receipt(
    pool: &DbPool,
    lease: &JobsLegacyInventoryPageLease,
    receipt: &JobsLegacyInventoryPageReceiptV3,
    now_ms: i64,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let lease_hash = workflow_command_lease_token_sha256(&lease.lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            if let Some(stored_digest) = tx
                .query_row(
                    "SELECT page_digest_sha256
                       FROM jobs_workflow_legacy_inventory_pages WHERE request_id = ?1",
                    params![lease.cleanup_request_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                if stored_digest != receipt.page_digest {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Replayed);
            }
            let generation = tx.query_row(
                "SELECT generation, completion_epoch
                   FROM jobs_workflow_legacy_inventory_generations
                  WHERE inventory_generation_id = ?1 AND query_digest_sha256 = ?2
                    AND state = 'scanning' AND scan_pass = ?3 AND page_index = ?4
                    AND request_epoch = ?5 AND fence = ?6 AND request_id = ?7
                    AND first_request_started_at_ms IS NOT NULL
                    AND lease_owner = ?8 AND lease_token_sha256 = ?9
                    AND lease_expires_at_ms = ?10 AND lease_expires_at_ms >= ?11",
                params![
                    lease.inventory_generation_id,
                    lease.query_digest,
                    lease.scan_pass,
                    lease.page_index,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    lease.cleanup_request_id,
                    lease.lease_owner,
                    lease_hash,
                    lease.lease_expires_at_ms,
                    now_ms,
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?;
            let input_token_ciphertext = lease
                .page_token
                .as_deref()
                .map(encrypt_payload)
                .transpose()?;
            let input_token_hmac = lease
                .page_token
                .as_deref()
                .map(workflow_cleanup_page_token_hmac)
                .transpose()?;
            let next_token_ciphertext = receipt
                .next_page_token
                .as_deref()
                .map(encrypt_payload)
                .transpose()?;
            let next_token_hmac = receipt
                .next_page_token
                .as_deref()
                .map(workflow_cleanup_page_token_hmac)
                .transpose()?;
            if let Some(next_token_hmac) = next_token_hmac.as_deref() {
                let repeated = input_token_hmac
                    .as_deref()
                    .is_some_and(|input| workflow_command_hmac_matches(input, next_token_hmac))
                    || tx.query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM jobs_workflow_legacy_inventory_pages
                             WHERE generation = ?1 AND completion_epoch = ?2
                               AND scan_pass = ?3
                               AND input_page_token_hmac_sha256 = ?4
                         )",
                        params![generation.0, generation.1, lease.scan_pass, next_token_hmac],
                        |row| row.get::<_, bool>(0),
                    )?;
                if repeated {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_inventory_pages (
                    generation, completion_epoch, scan_pass, page_index,
                    request_epoch, fence, request_id, predecessor_page_digest_sha256,
                    input_page_token_ciphertext, input_page_token_hmac_sha256,
                    next_page_token_ciphertext, next_page_token_hmac_sha256,
                    page_target_count, page_targets_digest_sha256, page_digest_sha256,
                    evidence_digest_sha256, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?15, ?16)",
                params![
                    generation.0,
                    generation.1,
                    lease.scan_pass,
                    lease.page_index,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    lease.cleanup_request_id,
                    lease.predecessor_page_digest,
                    input_token_ciphertext,
                    input_token_hmac,
                    next_token_ciphertext,
                    next_token_hmac,
                    receipt.targets.len() as i64,
                    receipt.targets_digest,
                    receipt.page_digest,
                    now_ms,
                ],
            )?;
            for target in &receipt.targets {
                let identity_hmac = workflow_cleanup_legacy_target_hmac(
                    &target.workflow_id,
                    &target.run_id,
                    &target.first_execution_run_id,
                )?;
                let target_lease = JobsLegacyTargetCleanupLease {
                    cleanup_request_id: "digest-only-request-v3".to_string(),
                    inventory_generation_id: lease.inventory_generation_id.clone(),
                    namespace: lease.namespace.clone(),
                    workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
                    visibility_cutoff_ms: lease.visibility_cutoff_ms,
                    query_digest: lease.query_digest.clone(),
                    scan_pass: lease.scan_pass,
                    workflow_id: target.workflow_id.clone(),
                    run_id: target.run_id.clone(),
                    first_execution_run_id: target.first_execution_run_id.clone(),
                    target_digest: String::new(),
                    cleanup_fence: 1,
                    observation_pass: 1,
                    proof_epoch: 1,
                    request_epoch: 1,
                    lease_owner: "digest".to_string(),
                    lease_token: "digest".to_string(),
                    lease_expires_at_ms: 1,
                };
                let target_digest = workflow_cleanup_legacy_target_digest(&target_lease)?;
                let workflow_hmac = workflow_command_hmac(
                    "legacy-workflow-id-index-v3",
                    &json!({"workflowId": target.workflow_id}),
                    2_048,
                )?;
                let run_hmac = workflow_command_hmac(
                    "legacy-run-id-index-v3",
                    &json!({"runId": target.run_id}),
                    1_024,
                )?;
                let first_run_hmac = workflow_command_hmac(
                    "legacy-first-run-id-index-v3",
                    &json!({"firstExecutionRunId": target.first_execution_run_id}),
                    1_024,
                )?;
                let existing = tx
                    .query_row(
                        "SELECT workflow_id_ciphertext, run_id_ciphertext,
                                first_execution_run_id_ciphertext,
                                workflow_id_hmac_sha256, run_id_hmac_sha256,
                                first_execution_run_id_hmac_sha256,
                                target_digest_sha256, raw_ids_scrubbed,
                                discovered_scan_pass
                           FROM jobs_workflow_legacy_targets
                          WHERE generation = ?1 AND target_identity_hmac_sha256 = ?2",
                        params![generation.0, identity_hmac],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, Option<String>>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, String>(5)?,
                                row.get::<_, String>(6)?,
                                row.get::<_, i64>(7)?,
                                row.get::<_, i64>(8)?,
                            ))
                        },
                    )
                    .optional()?;
                let observed_status = if target.status == "RUNNING" {
                    "running"
                } else {
                    "closed"
                };
                let target_state = if target.status == "RUNNING" {
                    "running_wait"
                } else {
                    "delete_pending"
                };
                if let Some(existing) = existing {
                    let mut stored_target_lease = target_lease.clone();
                    stored_target_lease.scan_pass = existing.8;
                    if !workflow_command_hmac_matches(&existing.3, &workflow_hmac)
                        || !workflow_command_hmac_matches(&existing.4, &run_hmac)
                        || !workflow_command_hmac_matches(&existing.5, &first_run_hmac)
                        || !workflow_command_hmac_matches(
                            &existing.6,
                            &workflow_cleanup_legacy_target_digest(&stored_target_lease)?,
                        )
                    {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                    if existing.7 == 1 {
                        if existing.0.is_some() || existing.1.is_some() || existing.2.is_some() {
                            return Err(JobsWorkflowCommandError::InvalidState.into());
                        }
                        if tx.execute(
                            "UPDATE jobs_workflow_legacy_targets
                                SET workflow_id_ciphertext = ?1,
                                    run_id_ciphertext = ?2,
                                    first_execution_run_id_ciphertext = ?3,
                                    raw_ids_scrubbed = 0,
                                    positive_reset_required = 0,
                                    observed_status = ?4, target_state = ?5,
                                    proof_epoch = proof_epoch + 1, observation_pass = 1,
                                    first_absence_observed_at_ms = NULL,
                                    request_id = NULL, first_request_started_at_ms = NULL,
                                    last_outcome_code = NULL, lease_owner = NULL,
                                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                    next_attempt_at_ms = ?6, absence_proved_at_ms = NULL,
                                    updated_at_ms = MAX(?6, updated_at_ms + 1)
                              WHERE generation = ?7 AND target_identity_hmac_sha256 = ?8
                                AND raw_ids_scrubbed = 1",
                            params![
                                encrypt_payload(&target.workflow_id)?,
                                encrypt_payload(&target.run_id)?,
                                encrypt_payload(&target.first_execution_run_id)?,
                                observed_status,
                                target_state,
                                now_ms,
                                generation.0,
                                identity_hmac,
                            ],
                        )? != 1
                        {
                            return Err(JobsWorkflowCommandError::InvalidState.into());
                        }
                    } else {
                        let (
                            Some(workflow_ciphertext),
                            Some(run_ciphertext),
                            Some(first_ciphertext),
                        ) = (
                            existing.0.as_deref(),
                            existing.1.as_deref(),
                            existing.2.as_deref(),
                        )
                        else {
                            return Err(JobsWorkflowCommandError::InvalidState.into());
                        };
                        if decrypt_payload(workflow_ciphertext)? != target.workflow_id
                            || decrypt_payload(run_ciphertext)? != target.run_id
                            || decrypt_payload(first_ciphertext)? != target.first_execution_run_id
                        {
                            return Err(JobsWorkflowCommandError::IdentityConflict.into());
                        }
                        tx.execute(
                            "UPDATE jobs_workflow_legacy_targets
                                SET observed_status = ?1, target_state = ?2,
                                    positive_reset_required = 0,
                                    proof_epoch = proof_epoch + 1, observation_pass = 1,
                                    first_absence_observed_at_ms = NULL,
                                    request_id = NULL, first_request_started_at_ms = NULL,
                                    last_outcome_code = NULL, lease_owner = NULL,
                                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                    next_attempt_at_ms = ?3, absence_proved_at_ms = NULL,
                                    updated_at_ms = MAX(?3, updated_at_ms + 1)
                              WHERE generation = ?4 AND target_identity_hmac_sha256 = ?5",
                            params![
                                observed_status,
                                target_state,
                                now_ms,
                                generation.0,
                                identity_hmac,
                            ],
                        )?;
                    }
                    continue;
                }
                tx.execute(
                    "INSERT INTO jobs_workflow_legacy_targets (
                        generation, target_identity_hmac_sha256,
                        workflow_id_ciphertext, workflow_id_hmac_sha256,
                        run_id_ciphertext, run_id_hmac_sha256,
                        first_execution_run_id_ciphertext,
                        first_execution_run_id_hmac_sha256, target_digest_sha256,
                        discovered_completion_epoch, discovered_scan_pass,
                        discovered_page_index, discovered_page_digest_sha256,
                        observed_status, target_state, observation_pass,
                        next_attempt_at_ms, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15, 1, ?16, ?16, ?16)",
                    params![
                        generation.0,
                        identity_hmac,
                        encrypt_payload(&target.workflow_id)?,
                        workflow_hmac,
                        encrypt_payload(&target.run_id)?,
                        run_hmac,
                        encrypt_payload(&target.first_execution_run_id)?,
                        first_run_hmac,
                        target_digest,
                        generation.1,
                        lease.scan_pass,
                        lease.page_index,
                        receipt.page_digest,
                        observed_status,
                        target_state,
                        now_ms,
                    ],
                )?;
            }
            let observed_count: i64 = tx.query_row(
                "SELECT COALESCE(SUM(page_target_count), 0)
                   FROM jobs_workflow_legacy_inventory_pages
                  WHERE generation = ?1 AND completion_epoch = ?2 AND scan_pass = ?3",
                params![generation.0, generation.1, lease.scan_pass],
                |row| row.get(0),
            )?;
            let next_page_index = lease
                .page_index
                .checked_add(1)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            let mut result = JobsWorkflowCleanupReceiptState::InventoryPageRecorded;
            if observed_count > 0 {
                let next_epoch = generation
                    .1
                    .checked_add(i64::from(receipt.exhausted))
                    .ok_or(JobsWorkflowCommandError::InvalidState)?;
                let (scan_pass, page_index, predecessor, state) = if receipt.exhausted {
                    (
                        1_i64,
                        0_i64,
                        workflow_cleanup_genesis_digest(&lease.query_digest, next_epoch)?,
                        "draining",
                    )
                } else {
                    (
                        lease.scan_pass,
                        next_page_index,
                        receipt.page_digest.clone(),
                        "draining",
                    )
                };
                tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = ?1, scan_pass = ?2, page_index = ?3,
                            predecessor_page_digest_sha256 = ?4,
                            page_token_ciphertext = ?5, page_token_hmac_sha256 = ?6,
                            request_epoch = 0, fence = 0, request_id = NULL,
                            first_request_started_at_ms = NULL,
                            last_outcome_code = NULL, lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?7, completion_epoch = ?8,
                            first_zero_observed_at_ms = NULL,
                            updated_at_ms = MAX(?7, updated_at_ms + 1)
                      WHERE generation = ?9",
                    params![
                        state,
                        scan_pass,
                        page_index,
                        predecessor,
                        if receipt.exhausted {
                            None::<String>
                        } else {
                            next_token_ciphertext
                        },
                        if receipt.exhausted {
                            None::<String>
                        } else {
                            next_token_hmac
                        },
                        now_ms,
                        next_epoch,
                        generation.0,
                    ],
                )?;
            } else if !receipt.exhausted {
                tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET page_index = ?1, predecessor_page_digest_sha256 = ?2,
                            page_token_ciphertext = ?3, page_token_hmac_sha256 = ?4,
                            request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'page_recorded', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?5,
                            updated_at_ms = MAX(?5, updated_at_ms + 1)
                      WHERE generation = ?6",
                    params![
                        next_page_index,
                        receipt.page_digest,
                        next_token_ciphertext,
                        next_token_hmac,
                        now_ms,
                        generation.0,
                    ],
                )?;
            } else if lease.page_index != 0 {
                let next_epoch = generation
                    .1
                    .checked_add(1)
                    .ok_or(JobsWorkflowCommandError::InvalidState)?;
                let genesis = workflow_cleanup_genesis_digest(&lease.query_digest, next_epoch)?;
                tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = 'scanning', scan_pass = 1, page_index = 0,
                            predecessor_page_digest_sha256 = ?1,
                            page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                            request_epoch = 0, fence = 0, request_id = NULL,
                            first_request_started_at_ms = NULL,
                            last_outcome_code = NULL, lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?2, completion_epoch = ?3,
                            first_zero_observed_at_ms = NULL,
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE generation = ?4",
                    params![genesis, now_ms, next_epoch, generation.0],
                )?;
            } else {
                let zero_digest = workflow_cleanup_sha256(
                    WORKFLOW_CLEANUP_ZERO_DIGEST_DOMAIN,
                    &json!({
                        "completionEpoch": generation.1,
                        "finalPageDigest": receipt.page_digest,
                        "inventoryGenerationId": lease.inventory_generation_id,
                        "scanPass": lease.scan_pass,
                    }),
                    "workflow legacy zero scan",
                )?;
                tx.execute(
                    "INSERT INTO jobs_workflow_legacy_zero_observations (
                        generation, completion_epoch, scan_pass, final_page_index,
                        final_page_digest_sha256, zero_digest_sha256, recorded_at_ms
                     ) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
                    params![
                        generation.0,
                        generation.1,
                        lease.scan_pass,
                        receipt.page_digest,
                        zero_digest,
                        now_ms,
                    ],
                )?;
                if lease.scan_pass == 1 {
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_inventory_generations
                            SET state = 'awaiting_second_scan',
                                first_zero_observed_at_ms = ?1,
                                request_id = NULL, first_request_started_at_ms = NULL,
                                last_outcome_code = 'page_recorded', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?1 + confirmation_age_ms,
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE generation = ?2",
                        params![now_ms, generation.0],
                    )?;
                } else {
                    let zeros = {
                        let mut statement = tx.prepare(
                            "SELECT zero_digest_sha256
                               FROM jobs_workflow_legacy_zero_observations
                              WHERE generation = ?1 AND completion_epoch = ?2
                              ORDER BY scan_pass",
                        )?;
                        let rows = statement
                            .query_map(params![generation.0, generation.1], |row| {
                                row.get::<_, String>(0)
                            })?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                        rows
                    };
                    if zeros.len() != 2 {
                        return Err(JobsWorkflowCommandError::InvalidState.into());
                    }
                    let completion_digest = workflow_cleanup_sha256(
                        WORKFLOW_CLEANUP_COMPLETION_DIGEST_DOMAIN,
                        &json!({
                            "completionEpoch": generation.1,
                            "inventoryGenerationId": lease.inventory_generation_id,
                            "queryDigest": lease.query_digest,
                            "zeroDigests": zeros,
                        }),
                        "workflow legacy inventory completion",
                    )?;
                    let revalidate_after_ms = now_ms
                        .checked_add(WORKFLOW_LEGACY_REVALIDATION_INTERVAL_MS)
                        .ok_or(JobsWorkflowCommandError::InvalidState)?;
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_inventory_pages
                            SET input_page_token_ciphertext = NULL,
                                next_page_token_ciphertext = NULL,
                                raw_ciphertexts_scrubbed = 1
                          WHERE generation = ?1 AND raw_ciphertexts_scrubbed = 0",
                        params![generation.0],
                    )?;
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET workflow_id_ciphertext = NULL,
                                run_id_ciphertext = NULL,
                                first_execution_run_id_ciphertext = NULL,
                                raw_ids_scrubbed = 1,
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE generation = ?2 AND target_state = 'absence_proved'
                            AND raw_ids_scrubbed = 0",
                        params![now_ms, generation.0],
                    )?;
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_inventory_generations
                            SET state = 'complete', completed_at_ms = ?1,
                                completion_digest_sha256 = ?2, revalidate_after_ms = ?3,
                                page_token_ciphertext = NULL,
                                page_token_hmac_sha256 = NULL,
                                request_id = NULL, first_request_started_at_ms = NULL,
                                last_outcome_code = 'page_recorded', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?3,
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE generation = ?4",
                        params![now_ms, completion_digest, revalidate_after_ms, generation.0,],
                    )?;
                    result = JobsWorkflowCleanupReceiptState::InventoryComplete;
                }
            }
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => record_jobs_legacy_inventory_page_receipt_postgres(
            pool,
            lease,
            receipt,
            now_ms,
            &lease_hash,
        ),
    })
}

fn record_jobs_legacy_inventory_page_receipt_postgres(
    pool: &DbPool,
    lease: &JobsLegacyInventoryPageLease,
    receipt: &JobsLegacyInventoryPageReceiptV3,
    _caller_now_ms: i64,
    lease_hash: &str,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let mut connection = pool.get_pg()?;
    let mut tx = connection.transaction()?;
    let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
    if let Some(row) = tx.query_opt(
        "SELECT page_digest_sha256
           FROM jobs_workflow_legacy_inventory_pages WHERE request_id = $1 FOR SHARE",
        &[&lease.cleanup_request_id],
    )? {
        if row.get::<_, String>(0) != receipt.page_digest {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        tx.commit()?;
        return Ok(JobsWorkflowCleanupReceiptState::Replayed);
    }
    let generation = tx.query_one(
        "SELECT generation, completion_epoch
           FROM jobs_workflow_legacy_inventory_generations
          WHERE inventory_generation_id = $1 AND query_digest_sha256 = $2
            AND state = 'scanning' AND scan_pass = $3 AND page_index = $4
            AND request_epoch = $5 AND fence = $6 AND request_id = $7
            AND first_request_started_at_ms IS NOT NULL
            AND lease_owner = $8 AND lease_token_sha256 = $9
            AND lease_expires_at_ms = $10 AND lease_expires_at_ms >= $11
          FOR UPDATE",
        &[
            &lease.inventory_generation_id,
            &lease.query_digest,
            &lease.scan_pass,
            &lease.page_index,
            &lease.request_epoch,
            &lease.cleanup_fence,
            &lease.cleanup_request_id,
            &lease.lease_owner,
            &lease_hash,
            &lease.lease_expires_at_ms,
            &now_ms,
        ],
    )?;
    let storage_generation: i64 = generation.get(0);
    let completion_epoch: i64 = generation.get(1);
    let input_token_ciphertext = lease
        .page_token
        .as_deref()
        .map(encrypt_payload)
        .transpose()?;
    let input_token_hmac = lease
        .page_token
        .as_deref()
        .map(workflow_cleanup_page_token_hmac)
        .transpose()?;
    let next_token_ciphertext = receipt
        .next_page_token
        .as_deref()
        .map(encrypt_payload)
        .transpose()?;
    let next_token_hmac = receipt
        .next_page_token
        .as_deref()
        .map(workflow_cleanup_page_token_hmac)
        .transpose()?;
    if let Some(next_token_hmac) = next_token_hmac.as_deref() {
        let repeated = input_token_hmac
            .as_deref()
            .is_some_and(|input| workflow_command_hmac_matches(input, next_token_hmac))
            || tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_workflow_legacy_inventory_pages
                         WHERE generation = $1 AND completion_epoch = $2
                           AND scan_pass = $3
                           AND input_page_token_hmac_sha256 = $4
                     )",
                    &[
                        &storage_generation,
                        &completion_epoch,
                        &lease.scan_pass,
                        &next_token_hmac,
                    ],
                )?
                .get::<_, bool>(0);
        if repeated {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
    }
    tx.execute(
        "INSERT INTO jobs_workflow_legacy_inventory_pages (
            generation, completion_epoch, scan_pass, page_index,
            request_epoch, fence, request_id, predecessor_page_digest_sha256,
            input_page_token_ciphertext, input_page_token_hmac_sha256,
            next_page_token_ciphertext, next_page_token_hmac_sha256,
            page_target_count, page_targets_digest_sha256, page_digest_sha256,
            evidence_digest_sha256, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $15, $16)",
        &[
            &storage_generation,
            &completion_epoch,
            &lease.scan_pass,
            &lease.page_index,
            &lease.request_epoch,
            &lease.cleanup_fence,
            &lease.cleanup_request_id,
            &lease.predecessor_page_digest,
            &input_token_ciphertext,
            &input_token_hmac,
            &next_token_ciphertext,
            &next_token_hmac,
            &(receipt.targets.len() as i64),
            &receipt.targets_digest,
            &receipt.page_digest,
            &now_ms,
        ],
    )?;
    for target in &receipt.targets {
        let identity_hmac = workflow_cleanup_legacy_target_hmac(
            &target.workflow_id,
            &target.run_id,
            &target.first_execution_run_id,
        )?;
        let target_lease = JobsLegacyTargetCleanupLease {
            cleanup_request_id: "digest-only-request-v3".to_string(),
            inventory_generation_id: lease.inventory_generation_id.clone(),
            namespace: lease.namespace.clone(),
            workflow_type: WORKFLOW_LEGACY_TYPE.to_string(),
            visibility_cutoff_ms: lease.visibility_cutoff_ms,
            query_digest: lease.query_digest.clone(),
            scan_pass: lease.scan_pass,
            workflow_id: target.workflow_id.clone(),
            run_id: target.run_id.clone(),
            first_execution_run_id: target.first_execution_run_id.clone(),
            target_digest: String::new(),
            cleanup_fence: 1,
            observation_pass: 1,
            proof_epoch: 1,
            request_epoch: 1,
            lease_owner: "digest".to_string(),
            lease_token: "digest".to_string(),
            lease_expires_at_ms: 1,
        };
        let target_digest = workflow_cleanup_legacy_target_digest(&target_lease)?;
        let workflow_hmac = workflow_command_hmac(
            "legacy-workflow-id-index-v3",
            &json!({"workflowId": target.workflow_id}),
            2_048,
        )?;
        let run_hmac = workflow_command_hmac(
            "legacy-run-id-index-v3",
            &json!({"runId": target.run_id}),
            1_024,
        )?;
        let first_run_hmac = workflow_command_hmac(
            "legacy-first-run-id-index-v3",
            &json!({"firstExecutionRunId": target.first_execution_run_id}),
            1_024,
        )?;
        let existing = tx.query_opt(
            "SELECT workflow_id_ciphertext, run_id_ciphertext,
                    first_execution_run_id_ciphertext,
                    workflow_id_hmac_sha256, run_id_hmac_sha256,
                    first_execution_run_id_hmac_sha256,
                    target_digest_sha256, raw_ids_scrubbed,
                    discovered_scan_pass
               FROM jobs_workflow_legacy_targets
              WHERE generation = $1 AND target_identity_hmac_sha256 = $2 FOR UPDATE",
            &[&storage_generation, &identity_hmac],
        )?;
        let observed_status = if target.status == "RUNNING" {
            "running"
        } else {
            "closed"
        };
        let target_state = if target.status == "RUNNING" {
            "running_wait"
        } else {
            "delete_pending"
        };
        if let Some(existing) = existing {
            let workflow_ciphertext = existing.get::<_, Option<String>>(0);
            let run_ciphertext = existing.get::<_, Option<String>>(1);
            let first_ciphertext = existing.get::<_, Option<String>>(2);
            let stored_workflow_hmac = existing.get::<_, String>(3);
            let stored_run_hmac = existing.get::<_, String>(4);
            let stored_first_run_hmac = existing.get::<_, String>(5);
            let stored_target_digest = existing.get::<_, String>(6);
            let raw_ids_scrubbed = existing.get::<_, bool>(7);
            let mut stored_target_lease = target_lease.clone();
            stored_target_lease.scan_pass = existing.get::<_, i64>(8);
            if !workflow_command_hmac_matches(&stored_workflow_hmac, &workflow_hmac)
                || !workflow_command_hmac_matches(&stored_run_hmac, &run_hmac)
                || !workflow_command_hmac_matches(&stored_first_run_hmac, &first_run_hmac)
                || !workflow_command_hmac_matches(
                    &stored_target_digest,
                    &workflow_cleanup_legacy_target_digest(&stored_target_lease)?,
                )
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if raw_ids_scrubbed {
                if workflow_ciphertext.is_some()
                    || run_ciphertext.is_some()
                    || first_ciphertext.is_some()
                {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                }
                let encrypted_workflow_id = encrypt_payload(&target.workflow_id)?;
                let encrypted_run_id = encrypt_payload(&target.run_id)?;
                let encrypted_first_run_id = encrypt_payload(&target.first_execution_run_id)?;
                if tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET workflow_id_ciphertext = $1,
                            run_id_ciphertext = $2,
                            first_execution_run_id_ciphertext = $3,
                            raw_ids_scrubbed = FALSE,
                            positive_reset_required = FALSE,
                            observed_status = $4, target_state = $5,
                            proof_epoch = proof_epoch + 1, observation_pass = 1,
                            first_absence_observed_at_ms = NULL,
                            request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = NULL, lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = $6, absence_proved_at_ms = NULL,
                            updated_at_ms = GREATEST($6, updated_at_ms + 1)
                      WHERE generation = $7 AND target_identity_hmac_sha256 = $8
                        AND raw_ids_scrubbed",
                    &[
                        &encrypted_workflow_id,
                        &encrypted_run_id,
                        &encrypted_first_run_id,
                        &observed_status,
                        &target_state,
                        &now_ms,
                        &storage_generation,
                        &identity_hmac,
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                }
            } else {
                let (Some(workflow_ciphertext), Some(run_ciphertext), Some(first_ciphertext)) = (
                    workflow_ciphertext.as_deref(),
                    run_ciphertext.as_deref(),
                    first_ciphertext.as_deref(),
                ) else {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                };
                if decrypt_payload(workflow_ciphertext)? != target.workflow_id
                    || decrypt_payload(run_ciphertext)? != target.run_id
                    || decrypt_payload(first_ciphertext)? != target.first_execution_run_id
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET observed_status = $1, target_state = $2,
                            positive_reset_required = FALSE,
                            proof_epoch = proof_epoch + 1, observation_pass = 1,
                            first_absence_observed_at_ms = NULL,
                            request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = NULL, lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = $3, absence_proved_at_ms = NULL,
                            updated_at_ms = GREATEST($3, updated_at_ms + 1)
                      WHERE generation = $4 AND target_identity_hmac_sha256 = $5",
                    &[
                        &observed_status,
                        &target_state,
                        &now_ms,
                        &storage_generation,
                        &identity_hmac,
                    ],
                )?;
            }
            continue;
        }
        tx.execute(
            "INSERT INTO jobs_workflow_legacy_targets (
                generation, target_identity_hmac_sha256,
                workflow_id_ciphertext, workflow_id_hmac_sha256,
                run_id_ciphertext, run_id_hmac_sha256,
                first_execution_run_id_ciphertext,
                first_execution_run_id_hmac_sha256, target_digest_sha256,
                discovered_completion_epoch, discovered_scan_pass,
                discovered_page_index, discovered_page_digest_sha256,
                observed_status, target_state, observation_pass,
                next_attempt_at_ms, created_at_ms, updated_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                $12, $13, $14, $15, 1, $16, $16, $16)",
            &[
                &storage_generation,
                &identity_hmac,
                &encrypt_payload(&target.workflow_id)?,
                &workflow_hmac,
                &encrypt_payload(&target.run_id)?,
                &run_hmac,
                &encrypt_payload(&target.first_execution_run_id)?,
                &first_run_hmac,
                &target_digest,
                &completion_epoch,
                &lease.scan_pass,
                &lease.page_index,
                &receipt.page_digest,
                &observed_status,
                &target_state,
                &now_ms,
            ],
        )?;
    }
    let observed_count: i64 = tx
        .query_one(
            "SELECT COALESCE(SUM(page_target_count), 0)::bigint
               FROM jobs_workflow_legacy_inventory_pages
              WHERE generation = $1 AND completion_epoch = $2 AND scan_pass = $3",
            &[&storage_generation, &completion_epoch, &lease.scan_pass],
        )?
        .get(0);
    let next_page_index = lease
        .page_index
        .checked_add(1)
        .ok_or(JobsWorkflowCommandError::InvalidState)?;
    let mut result = JobsWorkflowCleanupReceiptState::InventoryPageRecorded;
    if observed_count > 0 {
        let next_epoch = completion_epoch
            .checked_add(i64::from(receipt.exhausted))
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let (scan_pass, page_index, predecessor, state) = if receipt.exhausted {
            (
                1_i64,
                0_i64,
                workflow_cleanup_genesis_digest(&lease.query_digest, next_epoch)?,
                "draining",
            )
        } else {
            (
                lease.scan_pass,
                next_page_index,
                receipt.page_digest.clone(),
                "draining",
            )
        };
        let stored_next_ciphertext = if receipt.exhausted {
            None::<String>
        } else {
            next_token_ciphertext
        };
        let stored_next_hmac = if receipt.exhausted {
            None::<String>
        } else {
            next_token_hmac
        };
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = $1, scan_pass = $2, page_index = $3,
                    predecessor_page_digest_sha256 = $4,
                    page_token_ciphertext = $5, page_token_hmac_sha256 = $6,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL,
                    last_outcome_code = NULL, lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $7, completion_epoch = $8,
                    first_zero_observed_at_ms = NULL,
                    updated_at_ms = GREATEST($7, updated_at_ms + 1)
              WHERE generation = $9",
            &[
                &state,
                &scan_pass,
                &page_index,
                &predecessor,
                &stored_next_ciphertext,
                &stored_next_hmac,
                &now_ms,
                &next_epoch,
                &storage_generation,
            ],
        )?;
    } else if !receipt.exhausted {
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET page_index = $1, predecessor_page_digest_sha256 = $2,
                    page_token_ciphertext = $3, page_token_hmac_sha256 = $4,
                    request_id = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = 'page_recorded', lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $5,
                    updated_at_ms = GREATEST($5, updated_at_ms + 1)
              WHERE generation = $6",
            &[
                &next_page_index,
                &receipt.page_digest,
                &next_token_ciphertext,
                &next_token_hmac,
                &now_ms,
                &storage_generation,
            ],
        )?;
    } else if lease.page_index != 0 {
        let next_epoch = completion_epoch
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let genesis = workflow_cleanup_genesis_digest(&lease.query_digest, next_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 1, page_index = 0,
                    predecessor_page_digest_sha256 = $1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    first_request_started_at_ms = NULL,
                    last_outcome_code = NULL, lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $2, completion_epoch = $3,
                    first_zero_observed_at_ms = NULL,
                    updated_at_ms = GREATEST($2, updated_at_ms + 1)
              WHERE generation = $4",
            &[&genesis, &now_ms, &next_epoch, &storage_generation],
        )?;
    } else {
        let zero_digest = workflow_cleanup_sha256(
            WORKFLOW_CLEANUP_ZERO_DIGEST_DOMAIN,
            &json!({
                "completionEpoch": completion_epoch,
                "finalPageDigest": receipt.page_digest,
                "inventoryGenerationId": lease.inventory_generation_id,
                "scanPass": lease.scan_pass,
            }),
            "workflow legacy zero scan",
        )?;
        tx.execute(
            "INSERT INTO jobs_workflow_legacy_zero_observations (
                generation, completion_epoch, scan_pass, final_page_index,
                final_page_digest_sha256, zero_digest_sha256, recorded_at_ms
             ) VALUES ($1, $2, $3, 0, $4, $5, $6)",
            &[
                &storage_generation,
                &completion_epoch,
                &lease.scan_pass,
                &receipt.page_digest,
                &zero_digest,
                &now_ms,
            ],
        )?;
        if lease.scan_pass == 1 {
            tx.execute(
                "UPDATE jobs_workflow_legacy_inventory_generations
                    SET state = 'awaiting_second_scan', first_zero_observed_at_ms = $1,
                        request_id = NULL, first_request_started_at_ms = NULL,
                        last_outcome_code = 'page_recorded', lease_owner = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        next_attempt_at_ms = $1 + confirmation_age_ms,
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE generation = $2",
                &[&now_ms, &storage_generation],
            )?;
        } else {
            let zero_rows = tx.query(
                "SELECT zero_digest_sha256
                   FROM jobs_workflow_legacy_zero_observations
                  WHERE generation = $1 AND completion_epoch = $2 ORDER BY scan_pass",
                &[&storage_generation, &completion_epoch],
            )?;
            let zeros = zero_rows
                .iter()
                .map(|row| row.get::<_, String>(0))
                .collect::<Vec<_>>();
            if zeros.len() != 2 {
                return Err(JobsWorkflowCommandError::InvalidState.into());
            }
            let completion_digest = workflow_cleanup_sha256(
                WORKFLOW_CLEANUP_COMPLETION_DIGEST_DOMAIN,
                &json!({
                    "completionEpoch": completion_epoch,
                    "inventoryGenerationId": lease.inventory_generation_id,
                    "queryDigest": lease.query_digest,
                    "zeroDigests": zeros,
                }),
                "workflow legacy inventory completion",
            )?;
            let revalidate_after_ms = now_ms
                .checked_add(WORKFLOW_LEGACY_REVALIDATION_INTERVAL_MS)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            tx.execute(
                "UPDATE jobs_workflow_legacy_inventory_pages
                    SET input_page_token_ciphertext = NULL,
                        next_page_token_ciphertext = NULL,
                        raw_ciphertexts_scrubbed = TRUE
                  WHERE generation = $1 AND NOT raw_ciphertexts_scrubbed",
                &[&storage_generation],
            )?;
            tx.execute(
                "UPDATE jobs_workflow_legacy_targets
                    SET workflow_id_ciphertext = NULL,
                        run_id_ciphertext = NULL,
                        first_execution_run_id_ciphertext = NULL,
                        raw_ids_scrubbed = TRUE,
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE generation = $2 AND target_state = 'absence_proved'
                    AND NOT raw_ids_scrubbed",
                &[&now_ms, &storage_generation],
            )?;
            tx.execute(
                "UPDATE jobs_workflow_legacy_inventory_generations
                    SET state = 'complete', completed_at_ms = $1,
                        completion_digest_sha256 = $2, revalidate_after_ms = $3,
                        page_token_ciphertext = NULL,
                        page_token_hmac_sha256 = NULL,
                        request_id = NULL, first_request_started_at_ms = NULL,
                        last_outcome_code = 'page_recorded', lease_owner = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        next_attempt_at_ms = $3,
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE generation = $4",
                &[
                    &now_ms,
                    &completion_digest,
                    &revalidate_after_ms,
                    &storage_generation,
                ],
            )?;
            result = JobsWorkflowCleanupReceiptState::InventoryComplete;
        }
    }
    tx.commit()?;
    Ok(result)
}

fn legacy_receipt_observation_states(
    receipt: &JobsLegacyTargetReceiptV3,
) -> (&'static str, &'static str, &'static str, &'static str) {
    if receipt.outcome == "absence_observed" {
        return ("absence_proved", "not_found", "not_found", "not_found");
    }
    match receipt.reason.as_str() {
        "workflow_running" => ("running", "found", "not_checked", "not_checked"),
        "history_delete_pending" => ("retry", "not_found", "found", "not_checked"),
        "visibility_pending" => ("retry", "not_found", "not_found", "found"),
        _ => ("retry", "unavailable", "unavailable", "unavailable"),
    }
}

fn record_jobs_legacy_target_receipt(
    pool: &DbPool,
    lease: &JobsLegacyTargetCleanupLease,
    receipt: &JobsLegacyTargetReceiptV3,
    _caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let lease_hash = workflow_command_lease_token_sha256(&lease.lease_token);
    let target_hmac = workflow_cleanup_legacy_target_hmac(
        &lease.workflow_id,
        &lease.run_id,
        &lease.first_execution_run_id,
    )?;
    let (outcome, describe_state, history_state, visibility_state) =
        legacy_receipt_observation_states(receipt);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            if let Some(stored) = tx
                .query_row(
                    "SELECT evidence_digest_sha256, outcome
                       FROM jobs_workflow_legacy_target_observations
                      WHERE cleanup_request_id = ?1",
                    params![lease.cleanup_request_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
            {
                if stored.0 != receipt.evidence_digest || stored.1 != outcome {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Replayed);
            }
            let stored = tx.query_row(
                "SELECT target.generation, target.observation_pass,
                        legacy.confirmation_age_ms
                   FROM jobs_workflow_legacy_targets target
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = target.generation
                  WHERE target.target_identity_hmac_sha256 = ?1
                    AND target.target_digest_sha256 = ?2
                    AND target.proof_epoch = ?3
                    AND target.request_epoch = ?4 AND target.fence = ?5
                    AND target.request_id = ?6 AND target.observation_pass = ?7
                    AND target.first_request_started_at_ms IS NOT NULL
                    AND target.lease_owner = ?8 AND target.lease_token_sha256 = ?9
                    AND target.lease_expires_at_ms = ?10
                    AND target.lease_expires_at_ms >= ?11",
                params![
                    target_hmac,
                    lease.target_digest,
                    lease.proof_epoch,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    lease.cleanup_request_id,
                    lease.observation_pass,
                    lease.lease_owner,
                    lease_hash,
                    lease.lease_expires_at_ms,
                    now_ms,
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?;
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_target_observations (
                    id, generation, target_identity_hmac_sha256, target_digest_sha256,
                    proof_epoch, observation_pass, request_epoch, cleanup_fence,
                    cleanup_request_id,
                    outcome, describe_state, history_state, visibility_state,
                    evidence_digest_sha256, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15)",
                params![
                    format!("wflegacyobs-v3-{}", uuid::Uuid::new_v4()),
                    stored.0,
                    target_hmac,
                    lease.target_digest,
                    lease.proof_epoch,
                    lease.observation_pass,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    lease.cleanup_request_id,
                    outcome,
                    describe_state,
                    history_state,
                    visibility_state,
                    receipt.evidence_digest,
                    now_ms,
                ],
            )?;
            let result = if receipt.outcome == "pending" {
                let positive_presence = receipt.reason != "temporal_unavailable";
                let state = if receipt.reason == "workflow_running" {
                    "running_wait"
                } else {
                    "delete_pending"
                };
                let updated = if positive_presence {
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET target_state = ?1, proof_epoch = proof_epoch + 1,
                                positive_reset_required = 0,
                                observation_pass = 1,
                                first_absence_observed_at_ms = NULL,
                                absence_proved_at_ms = NULL, request_id = NULL,
                                first_request_started_at_ms = NULL,
                                last_outcome_code = ?2, lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?3,
                                updated_at_ms = MAX(?4, updated_at_ms + 1)
                          WHERE generation = ?5 AND target_identity_hmac_sha256 = ?6
                            AND proof_epoch = ?7 AND positive_reset_required = 1",
                        params![
                            state,
                            if receipt.reason == "workflow_running" {
                                "running"
                            } else {
                                "retry"
                            },
                            now_ms + WORKFLOW_CLEANUP_RETRY_MS,
                            now_ms,
                            stored.0,
                            target_hmac,
                            lease.proof_epoch,
                        ],
                    )?
                } else {
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET request_id = NULL, first_request_started_at_ms = NULL,
                                last_outcome_code = 'retry', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?1,
                                updated_at_ms = MAX(?2, updated_at_ms + 1)
                          WHERE generation = ?3 AND target_identity_hmac_sha256 = ?4
                            AND proof_epoch = ?5",
                        params![
                            now_ms + WORKFLOW_CLEANUP_RETRY_MS,
                            now_ms,
                            stored.0,
                            target_hmac,
                            lease.proof_epoch,
                        ],
                    )?
                };
                if updated != 1 {
                    anyhow::bail!("workflow legacy proof epoch is not current")
                }
                JobsWorkflowCleanupReceiptState::Pending
            } else if stored.1 == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = 'absence_pending', observation_pass = 2,
                            first_absence_observed_at_ms = ?1, request_id = NULL,
                            first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_proved', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?1 + ?2,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE generation = ?3 AND target_identity_hmac_sha256 = ?4
                        AND proof_epoch = ?5",
                    params![now_ms, stored.2, stored.0, target_hmac, lease.proof_epoch],
                )?;
                JobsWorkflowCleanupReceiptState::ObservationRecorded
            } else {
                tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = 'absence_proved', absence_proved_at_ms = ?1,
                            request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_proved', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?1,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE generation = ?2 AND target_identity_hmac_sha256 = ?3
                        AND proof_epoch = ?4",
                    params![now_ms, stored.0, target_hmac, lease.proof_epoch],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = 'scanning', next_attempt_at_ms = ?1,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE generation = ?2 AND state = 'draining'
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_legacy_targets target
                           WHERE target.generation = ?2
                             AND target.target_state <> 'absence_proved'
                        )",
                    params![now_ms, stored.0],
                )?;
                JobsWorkflowCleanupReceiptState::TargetComplete
            };
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            if let Some(row) = tx.query_opt(
                "SELECT evidence_digest_sha256, outcome
                   FROM jobs_workflow_legacy_target_observations
                  WHERE cleanup_request_id = $1 FOR SHARE",
                &[&lease.cleanup_request_id],
            )? {
                if row.get::<_, String>(0) != receipt.evidence_digest
                    || row.get::<_, String>(1) != outcome
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Replayed);
            }
            // Keep the global -> generation -> target lock order explicit. A
            // multi-relation FOR UPDATE is planner-dependent and can deadlock
            // the hard-delete and inventory paths, which use this same order.
            let legacy = tx.query_one(
                "SELECT legacy.generation, legacy.confirmation_age_ms
                   FROM jobs_workflow_legacy_inventory_head head
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = head.generation
                    AND legacy.inventory_generation_id = head.inventory_generation_id
                    AND legacy.query_digest_sha256 = head.query_digest_sha256
                  WHERE head.singleton_id = 1
                    AND legacy.inventory_generation_id = $1
                    AND legacy.query_digest_sha256 = $2
                  FOR UPDATE OF head, legacy",
                &[&lease.inventory_generation_id, &lease.query_digest],
            )?;
            let storage_generation: i64 = legacy.get(0);
            let confirmation_age_ms: i64 = legacy.get(1);
            let row = tx.query_one(
                "SELECT target.observation_pass
                   FROM jobs_workflow_legacy_targets target
                  WHERE target.generation = $1
                    AND target.target_identity_hmac_sha256 = $2
                    AND target.target_digest_sha256 = $3
                    AND target.proof_epoch = $4
                    AND target.request_epoch = $5 AND target.fence = $6
                    AND target.request_id = $7 AND target.observation_pass = $8
                    AND target.first_request_started_at_ms IS NOT NULL
                    AND target.lease_owner = $9 AND target.lease_token_sha256 = $10
                    AND target.lease_expires_at_ms = $11
                    AND target.lease_expires_at_ms >= $12 FOR UPDATE OF target",
                &[
                    &storage_generation,
                    &target_hmac,
                    &lease.target_digest,
                    &lease.proof_epoch,
                    &lease.request_epoch,
                    &lease.cleanup_fence,
                    &lease.cleanup_request_id,
                    &lease.observation_pass,
                    &lease.lease_owner,
                    &lease_hash,
                    &lease.lease_expires_at_ms,
                    &now_ms,
                ],
            )?;
            let observation_pass: i64 = row.get(0);
            tx.execute(
                "INSERT INTO jobs_workflow_legacy_target_observations (
                    id, generation, target_identity_hmac_sha256, target_digest_sha256,
                    proof_epoch, observation_pass, request_epoch, cleanup_fence,
                    cleanup_request_id,
                    outcome, describe_state, history_state, visibility_state,
                    evidence_digest_sha256, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                    $13, $14, $15)",
                &[
                    &format!("wflegacyobs-v3-{}", uuid::Uuid::new_v4()),
                    &storage_generation,
                    &target_hmac,
                    &lease.target_digest,
                    &lease.proof_epoch,
                    &lease.observation_pass,
                    &lease.request_epoch,
                    &lease.cleanup_fence,
                    &lease.cleanup_request_id,
                    &outcome,
                    &describe_state,
                    &history_state,
                    &visibility_state,
                    &receipt.evidence_digest,
                    &now_ms,
                ],
            )?;
            let result = if receipt.outcome == "pending" {
                let positive_presence = receipt.reason != "temporal_unavailable";
                let state = if receipt.reason == "workflow_running" {
                    "running_wait"
                } else {
                    "delete_pending"
                };
                let last_outcome = if receipt.reason == "workflow_running" {
                    "running"
                } else {
                    "retry"
                };
                let updated = if positive_presence {
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET target_state = $1, proof_epoch = proof_epoch + 1,
                                positive_reset_required = FALSE,
                                observation_pass = 1,
                                first_absence_observed_at_ms = NULL,
                                absence_proved_at_ms = NULL, request_id = NULL,
                                first_request_started_at_ms = NULL,
                                last_outcome_code = $2, lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = $3,
                                updated_at_ms = GREATEST($4, updated_at_ms + 1)
                          WHERE generation = $5 AND target_identity_hmac_sha256 = $6
                            AND proof_epoch = $7 AND positive_reset_required",
                        &[
                            &state,
                            &last_outcome,
                            &(now_ms + WORKFLOW_CLEANUP_RETRY_MS),
                            &now_ms,
                            &storage_generation,
                            &target_hmac,
                            &lease.proof_epoch,
                        ],
                    )?
                } else {
                    tx.execute(
                        "UPDATE jobs_workflow_legacy_targets
                            SET request_id = NULL, first_request_started_at_ms = NULL,
                                last_outcome_code = 'retry', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = $1,
                                updated_at_ms = GREATEST($2, updated_at_ms + 1)
                          WHERE generation = $3 AND target_identity_hmac_sha256 = $4
                            AND proof_epoch = $5",
                        &[
                            &(now_ms + WORKFLOW_CLEANUP_RETRY_MS),
                            &now_ms,
                            &storage_generation,
                            &target_hmac,
                            &lease.proof_epoch,
                        ],
                    )?
                };
                if updated != 1 {
                    anyhow::bail!("workflow legacy proof epoch is not current")
                }
                JobsWorkflowCleanupReceiptState::Pending
            } else if observation_pass == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = 'absence_pending', observation_pass = 2,
                            first_absence_observed_at_ms = $1, request_id = NULL,
                            first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_proved', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = $1::BIGINT + $2::BIGINT,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE generation = $3 AND target_identity_hmac_sha256 = $4
                        AND proof_epoch = $5",
                    &[
                        &now_ms,
                        &confirmation_age_ms,
                        &storage_generation,
                        &target_hmac,
                        &lease.proof_epoch,
                    ],
                )?;
                JobsWorkflowCleanupReceiptState::ObservationRecorded
            } else {
                tx.execute(
                    "UPDATE jobs_workflow_legacy_targets
                        SET target_state = 'absence_proved', absence_proved_at_ms = $1,
                            request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_proved', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = $1,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE generation = $2 AND target_identity_hmac_sha256 = $3
                        AND proof_epoch = $4",
                    &[
                        &now_ms,
                        &storage_generation,
                        &target_hmac,
                        &lease.proof_epoch,
                    ],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_legacy_inventory_generations
                        SET state = 'scanning', next_attempt_at_ms = $1,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE generation = $2 AND state = 'draining'
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_legacy_targets target
                           WHERE target.generation = $2
                             AND target.target_state <> 'absence_proved'
                        )",
                    &[&now_ms, &storage_generation],
                )?;
                JobsWorkflowCleanupReceiptState::TargetComplete
            };
            tx.commit()?;
            Ok(result)
        }
    })
}

fn record_jobs_v2_target_receipt(
    pool: &DbPool,
    lease: &JobsV2TargetCleanupLease,
    receipt: &JobsV2TargetReceiptV3,
    now_ms: i64,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let lease_hash = workflow_command_lease_token_sha256(&lease.lease_token);
    let response_run_set_digest =
        workflow_cleanup_v2_known_run_set_digest(&lease.workflow_id, &receipt.run_ids)?;
    let response_first_ciphertext = receipt
        .first_execution_run_id
        .as_deref()
        .map(encrypt_payload)
        .transpose()?;
    let response_first_hmac = receipt
        .first_execution_run_id
        .as_deref()
        .map(|first| {
            workflow_command_hmac(
                "v2-cleanup-first-run-index-v3",
                &json!({"firstExecutionRunId": first, "workflowId": lease.workflow_id}),
                1_024,
            )
        })
        .transpose()?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            if let Some(stored) = tx
                .query_row(
                    "SELECT evidence_digest_sha256, outcome, reason,
                            response_run_set_digest_sha256,
                            first_execution_run_id_ciphertext
                       FROM jobs_workflow_cleanup_v2_receipts
                      WHERE cleanup_request_id = ?1",
                    params![lease.cleanup_request_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()?
            {
                let stored_first = stored.4.as_deref().map(decrypt_payload).transpose()?;
                if stored.0 != receipt.evidence_digest
                    || stored.1 != receipt.outcome
                    || stored.2 != receipt.reason
                    || stored.3 != response_run_set_digest
                    || stored_first != receipt.first_execution_run_id
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Replayed);
            }
            let stored = tx.query_row(
                "SELECT authority.account_id, authority.workflow_cleanup_generation,
                        authority.known_run_epoch, authority.observation_pass,
                        legacy.confirmation_age_ms
                   FROM jobs_workflow_cleanup_v2_target_authorities authority
                   JOIN jobs_workflow_cleanup_account_bindings binding
                     ON binding.account_id = authority.account_id
                    AND binding.workflow_cleanup_generation =
                        authority.workflow_cleanup_generation
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = binding.legacy_generation
                  WHERE authority.workflow_id = ?1 AND authority.target_digest_sha256 = ?2
                    AND authority.request_epoch = ?3 AND authority.cleanup_fence = ?4
                    AND authority.cleanup_request_id = ?5
                    AND authority.observation_pass = ?6
                    AND authority.first_request_started_at_ms IS NOT NULL
                    AND authority.lease_owner = ?7 AND authority.lease_token_sha256 = ?8
                    AND authority.lease_expires_at_ms = ?9
                    AND authority.lease_expires_at_ms >= ?10",
                params![
                    lease.workflow_id,
                    lease.target_digest,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    lease.cleanup_request_id,
                    lease.observation_pass,
                    lease.lease_owner,
                    lease_hash,
                    lease.lease_expires_at_ms,
                    now_ms,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )?;
            tx.execute(
                "INSERT INTO jobs_workflow_cleanup_v2_receipts (
                    account_id, workflow_cleanup_generation, workflow_id,
                    cleanup_request_id, request_epoch, cleanup_fence, known_run_epoch,
                    target_digest_sha256, outcome, reason,
                    first_execution_run_id_ciphertext,
                    first_execution_run_id_hmac_sha256,
                    response_run_set_digest_sha256, evidence_digest_sha256, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15)",
                params![
                    stored.0,
                    stored.1,
                    lease.workflow_id,
                    lease.cleanup_request_id,
                    lease.request_epoch,
                    lease.cleanup_fence,
                    stored.2,
                    lease.target_digest,
                    receipt.outcome,
                    receipt.reason,
                    response_first_ciphertext,
                    response_first_hmac,
                    response_run_set_digest,
                    receipt.evidence_digest,
                    now_ms,
                ],
            )?;
            let identity_expanded = receipt.run_ids != lease.known_run_ids
                || receipt.first_execution_run_id != lease.first_execution_run_id;
            if identity_expanded {
                for run_id in &receipt.run_ids {
                    let run_hmac = workflow_command_hmac(
                        "v2-cleanup-known-run-index-v3",
                        &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                        1_024,
                    )?;
                    if let Some(existing) = tx
                        .query_row(
                            "SELECT run_id_ciphertext
                               FROM jobs_workflow_cleanup_v2_known_runs
                              WHERE account_id = ?1 AND workflow_cleanup_generation = ?2
                                AND workflow_id = ?3 AND run_id_hmac_sha256 = ?4",
                            params![stored.0, stored.1, lease.workflow_id, run_hmac],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                    {
                        if decrypt_payload(&existing)? != *run_id {
                            return Err(JobsWorkflowCommandError::IdentityConflict.into());
                        }
                        continue;
                    }
                    let run_digest = workflow_cleanup_sha256(
                        WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
                        &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                        "workflow v2 known run identity",
                    )?;
                    tx.execute(
                        "INSERT INTO jobs_workflow_cleanup_v2_known_runs (
                            account_id, workflow_cleanup_generation, workflow_id,
                            run_id_hmac_sha256, run_id_ciphertext,
                            discovered_request_epoch, discovered_cleanup_fence,
                            run_identity_digest_sha256, created_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            stored.0,
                            stored.1,
                            lease.workflow_id,
                            run_hmac,
                            encrypt_payload(run_id)?,
                            lease.request_epoch,
                            lease.cleanup_fence,
                            run_digest,
                            now_ms,
                        ],
                    )?;
                }
                let first_run = receipt
                    .first_execution_run_id
                    .as_deref()
                    .ok_or(JobsWorkflowCommandError::InvalidState)?;
                if lease.first_execution_run_id.is_none() {
                    tx.execute(
                        "INSERT INTO jobs_workflow_execution_cleanup_observations (
                            id, account_id, workflow_id, generation,
                            target_set_hmac_sha256, observation_kind,
                            observed_execution_run_id, cleanup_fence,
                            cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'identity_confirmed', ?6, ?7,
                            ?8, ?9, ?10)",
                        params![
                            format!("wfv2identity-v3-{}", uuid::Uuid::new_v4()),
                            stored.0,
                            lease.workflow_id,
                            stored.1,
                            lease.target_set_digest,
                            first_run,
                            lease.cleanup_fence,
                            lease.cleanup_request_id,
                            receipt.evidence_digest,
                            now_ms,
                        ],
                    )?;
                    tx.execute(
                        "UPDATE jobs_workflow_cleanup_targets
                            SET first_execution_run_id = ?1,
                                target_state = 'cleanup_required',
                                updated_at_ms = MAX(?2, updated_at_ms + 1)
                          WHERE account_id = ?3 AND generation = ?4 AND workflow_id = ?5
                            AND first_execution_run_id IS NULL",
                        params![first_run, now_ms, stored.0, stored.1, lease.workflow_id],
                    )?;
                }
                let next_target_digest =
                    workflow_cleanup_v2_target_digest_from_lease(lease, Some(first_run))?;
                if tx.execute(
                    "UPDATE jobs_workflow_cleanup_v2_target_authorities
                        SET known_run_epoch = known_run_epoch + 1,
                            positive_reset_required = 0,
                            known_run_set_digest_sha256 = ?1,
                            target_digest_sha256 = ?2, observation_pass = 1,
                            first_absence_observed_at_ms = NULL,
                            cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'pending', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?3,
                            updated_at_ms = MAX(?3, updated_at_ms + 1)
                      WHERE account_id = ?4 AND workflow_cleanup_generation = ?5
                        AND workflow_id = ?6 AND positive_reset_required = 1",
                    params![
                        response_run_set_digest,
                        next_target_digest,
                        now_ms,
                        stored.0,
                        stored.1,
                        lease.workflow_id,
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                }
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND generation = ?3 AND workflow_id = ?4",
                    params![now_ms, stored.0, stored.1, lease.workflow_id],
                )?;
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Pending);
            }
            if receipt.outcome == "pending" {
                let positive_presence = receipt.reason != "temporal_unavailable";
                let updated = if positive_presence {
                    tx.execute(
                        "UPDATE jobs_workflow_cleanup_v2_target_authorities
                            SET known_run_epoch = known_run_epoch + 1,
                                positive_reset_required = 0,
                                observation_pass = 1,
                                first_absence_observed_at_ms = NULL,
                                cleanup_request_id = NULL,
                                first_request_started_at_ms = NULL,
                                last_outcome_code = 'pending', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?1,
                                updated_at_ms = MAX(?2, updated_at_ms + 1)
                          WHERE account_id = ?3 AND workflow_cleanup_generation = ?4
                            AND workflow_id = ?5 AND positive_reset_required = 1",
                        params![
                            now_ms + WORKFLOW_CLEANUP_RETRY_MS,
                            now_ms,
                            stored.0,
                            stored.1,
                            lease.workflow_id,
                        ],
                    )?
                } else {
                    tx.execute(
                        "UPDATE jobs_workflow_cleanup_v2_target_authorities
                            SET cleanup_request_id = NULL,
                                first_request_started_at_ms = NULL,
                                last_outcome_code = 'pending', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?1,
                                updated_at_ms = MAX(?2, updated_at_ms + 1)
                          WHERE account_id = ?3 AND workflow_cleanup_generation = ?4
                            AND workflow_id = ?5",
                        params![
                            now_ms + WORKFLOW_CLEANUP_RETRY_MS,
                            now_ms,
                            stored.0,
                            stored.1,
                            lease.workflow_id,
                        ],
                    )?
                };
                if updated != 1 {
                    anyhow::bail!("workflow v2 proof epoch is not current")
                }
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND generation = ?3 AND workflow_id = ?4",
                    params![now_ms, stored.0, stored.1, lease.workflow_id],
                )?;
                tx.commit()?;
                return Ok(JobsWorkflowCleanupReceiptState::Pending);
            }
            let subjects = if receipt.run_ids.is_empty() {
                vec![("workflow", "0".repeat(64))]
            } else {
                receipt
                    .run_ids
                    .iter()
                    .map(|run_id| {
                        Ok((
                            "run",
                            workflow_command_hmac(
                                "v2-cleanup-known-run-index-v3",
                                &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                                1_024,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            for (subject_kind, run_hmac) in subjects {
                tx.execute(
                    "INSERT INTO jobs_workflow_cleanup_v2_run_observations (
                        account_id, workflow_cleanup_generation, workflow_id,
                        known_run_epoch, observation_pass, subject_kind,
                        run_id_hmac_sha256, target_digest_sha256, request_epoch,
                        cleanup_fence, cleanup_request_id, evidence_digest_sha256,
                        describe_state, history_state, visibility_state, recorded_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                        ?12, 'not_found', 'not_found', 'not_found', ?13)",
                    params![
                        stored.0,
                        stored.1,
                        lease.workflow_id,
                        stored.2,
                        stored.3,
                        subject_kind,
                        run_hmac,
                        lease.target_digest,
                        lease.request_epoch,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        receipt.evidence_digest,
                        now_ms,
                    ],
                )?;
            }
            let result = if stored.3 == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_v2_target_authorities
                        SET observation_pass = 2, first_absence_observed_at_ms = ?1,
                            cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_observed', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?1 + ?2,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?3 AND workflow_cleanup_generation = ?4
                        AND workflow_id = ?5",
                    params![now_ms, stored.4, stored.0, stored.1, lease.workflow_id],
                )?;
                JobsWorkflowCleanupReceiptState::ObservationRecorded
            } else {
                tx.execute(
                    "INSERT INTO jobs_workflow_execution_cleanup_observations (
                        id, account_id, workflow_id, generation,
                        target_set_hmac_sha256, observation_kind,
                        observed_execution_run_id, cleanup_fence,
                        cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'absence_proved', ?6, ?7, ?8,
                        ?9, ?10)",
                    params![
                        format!("wfv2absence-v3-{}", uuid::Uuid::new_v4()),
                        stored.0,
                        lease.workflow_id,
                        stored.1,
                        lease.target_set_digest,
                        lease.first_execution_run_id,
                        lease.cleanup_fence,
                        lease.cleanup_request_id,
                        receipt.evidence_digest,
                        now_ms,
                    ],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET target_state = 'absence_proved', absence_proved_at_ms = ?1,
                            lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND generation = ?3 AND workflow_id = ?4",
                    params![now_ms, stored.0, stored.1, lease.workflow_id],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_executions
                        SET lifecycle_state = 'absent', cleanup_state = 'absence_proved',
                            absence_proved_at_ms = ?1,
                            cleanup_lease_owner = NULL,
                            cleanup_lease_token_sha256 = NULL,
                            cleanup_lease_expires_at_ms = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND workflow_id = ?3
                        AND deletion_target_generation = ?4",
                    params![now_ms, stored.0, lease.workflow_id, stored.1],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_v2_target_authorities
                        SET cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                            last_outcome_code = 'absence_observed', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = ?1,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND workflow_cleanup_generation = ?3
                        AND workflow_id = ?4",
                    params![now_ms, stored.0, stored.1, lease.workflow_id],
                )?;
                JobsWorkflowCleanupReceiptState::TargetComplete
            };
            if stored.3 == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND generation = ?3 AND workflow_id = ?4",
                    params![now_ms, stored.0, stored.1, lease.workflow_id],
                )?;
            }
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => record_jobs_v2_target_receipt_postgres(
            pool,
            lease,
            receipt,
            now_ms,
            &lease_hash,
            &response_run_set_digest,
            response_first_ciphertext.as_deref(),
            response_first_hmac.as_deref(),
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn record_jobs_v2_target_receipt_postgres(
    pool: &DbPool,
    lease: &JobsV2TargetCleanupLease,
    receipt: &JobsV2TargetReceiptV3,
    _caller_now_ms: i64,
    lease_hash: &str,
    response_run_set_digest: &str,
    response_first_ciphertext: Option<&str>,
    response_first_hmac: Option<&str>,
) -> Result<JobsWorkflowCleanupReceiptState> {
    let mut connection = pool.get_pg()?;
    let mut tx = connection.transaction()?;
    let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
    if let Some(row) = tx.query_opt(
        "SELECT evidence_digest_sha256, outcome, reason,
                response_run_set_digest_sha256, first_execution_run_id_ciphertext
           FROM jobs_workflow_cleanup_v2_receipts
          WHERE cleanup_request_id = $1 FOR SHARE",
        &[&lease.cleanup_request_id],
    )? {
        let stored_ciphertext: Option<String> = row.get(4);
        let stored_first = stored_ciphertext
            .as_deref()
            .map(decrypt_payload)
            .transpose()?;
        if row.get::<_, String>(0) != receipt.evidence_digest
            || row.get::<_, String>(1) != receipt.outcome
            || row.get::<_, String>(2) != receipt.reason
            || row.get::<_, String>(3) != response_run_set_digest
            || stored_first != receipt.first_execution_run_id
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        tx.commit()?;
        return Ok(JobsWorkflowCleanupReceiptState::Replayed);
    }
    let row = tx.query_one(
        "SELECT authority.account_id, authority.workflow_cleanup_generation,
                authority.known_run_epoch, authority.observation_pass,
                legacy.confirmation_age_ms
           FROM jobs_workflow_cleanup_v2_target_authorities authority
           JOIN jobs_workflow_cleanup_account_bindings binding
             ON binding.account_id = authority.account_id
            AND binding.workflow_cleanup_generation = authority.workflow_cleanup_generation
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = binding.legacy_generation
          WHERE authority.workflow_id = $1 AND authority.target_digest_sha256 = $2
            AND authority.request_epoch = $3 AND authority.cleanup_fence = $4
            AND authority.cleanup_request_id = $5 AND authority.observation_pass = $6
            AND authority.first_request_started_at_ms IS NOT NULL
            AND authority.lease_owner = $7 AND authority.lease_token_sha256 = $8
            AND authority.lease_expires_at_ms = $9
            AND authority.lease_expires_at_ms >= $10
          FOR UPDATE OF authority",
        &[
            &lease.workflow_id,
            &lease.target_digest,
            &lease.request_epoch,
            &lease.cleanup_fence,
            &lease.cleanup_request_id,
            &lease.observation_pass,
            &lease.lease_owner,
            &lease_hash,
            &lease.lease_expires_at_ms,
            &now_ms,
        ],
    )?;
    let account_id: String = row.get(0);
    let generation: i64 = row.get(1);
    let known_run_epoch: i64 = row.get(2);
    let observation_pass: i64 = row.get(3);
    let confirmation_age_ms: i64 = row.get(4);
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_v2_receipts (
            account_id, workflow_cleanup_generation, workflow_id,
            cleanup_request_id, request_epoch, cleanup_fence, known_run_epoch,
            target_digest_sha256, outcome, reason,
            first_execution_run_id_ciphertext, first_execution_run_id_hmac_sha256,
            response_run_set_digest_sha256, evidence_digest_sha256, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15)",
        &[
            &account_id,
            &generation,
            &lease.workflow_id,
            &lease.cleanup_request_id,
            &lease.request_epoch,
            &lease.cleanup_fence,
            &known_run_epoch,
            &lease.target_digest,
            &receipt.outcome,
            &receipt.reason,
            &response_first_ciphertext,
            &response_first_hmac,
            &response_run_set_digest,
            &receipt.evidence_digest,
            &now_ms,
        ],
    )?;
    let identity_expanded = receipt.run_ids != lease.known_run_ids
        || receipt.first_execution_run_id != lease.first_execution_run_id;
    if identity_expanded {
        for run_id in &receipt.run_ids {
            let run_hmac = workflow_command_hmac(
                "v2-cleanup-known-run-index-v3",
                &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                1_024,
            )?;
            if let Some(existing) = tx.query_opt(
                "SELECT run_id_ciphertext FROM jobs_workflow_cleanup_v2_known_runs
                  WHERE account_id = $1 AND workflow_cleanup_generation = $2
                    AND workflow_id = $3 AND run_id_hmac_sha256 = $4 FOR SHARE",
                &[&account_id, &generation, &lease.workflow_id, &run_hmac],
            )? {
                if decrypt_payload(existing.get::<_, String>(0).as_str())? != *run_id {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                continue;
            }
            let run_digest = workflow_cleanup_sha256(
                WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
                &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                "workflow v2 known run identity",
            )?;
            tx.execute(
                "INSERT INTO jobs_workflow_cleanup_v2_known_runs (
                    account_id, workflow_cleanup_generation, workflow_id,
                    run_id_hmac_sha256, run_id_ciphertext, discovered_request_epoch,
                    discovered_cleanup_fence, run_identity_digest_sha256, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &account_id,
                    &generation,
                    &lease.workflow_id,
                    &run_hmac,
                    &encrypt_payload(run_id)?,
                    &lease.request_epoch,
                    &lease.cleanup_fence,
                    &run_digest,
                    &now_ms,
                ],
            )?;
        }
        let first_run = receipt
            .first_execution_run_id
            .as_deref()
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        if lease.first_execution_run_id.is_none() {
            tx.execute(
                "INSERT INTO jobs_workflow_execution_cleanup_observations (
                    id, account_id, workflow_id, generation, target_set_hmac_sha256,
                    observation_kind, observed_execution_run_id, cleanup_fence,
                    cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'identity_confirmed', $6, $7, $8,
                    $9, $10)",
                &[
                    &format!("wfv2identity-v3-{}", uuid::Uuid::new_v4()),
                    &account_id,
                    &lease.workflow_id,
                    &generation,
                    &lease.target_set_digest,
                    &first_run,
                    &lease.cleanup_fence,
                    &lease.cleanup_request_id,
                    &receipt.evidence_digest,
                    &now_ms,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET first_execution_run_id = $1, target_state = 'cleanup_required',
                        updated_at_ms = GREATEST($2, updated_at_ms + 1)
                  WHERE account_id = $3 AND generation = $4 AND workflow_id = $5
                    AND first_execution_run_id IS NULL",
                &[
                    &first_run,
                    &now_ms,
                    &account_id,
                    &generation,
                    &lease.workflow_id,
                ],
            )?;
        }
        let next_target_digest =
            workflow_cleanup_v2_target_digest_from_lease(lease, Some(first_run))?;
        if tx.execute(
            "UPDATE jobs_workflow_cleanup_v2_target_authorities
                SET known_run_epoch = known_run_epoch + 1,
                    positive_reset_required = FALSE,
                    known_run_set_digest_sha256 = $1, target_digest_sha256 = $2,
                    observation_pass = 1, first_absence_observed_at_ms = NULL,
                    cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = 'pending', lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $3,
                    updated_at_ms = GREATEST($3, updated_at_ms + 1)
              WHERE account_id = $4 AND workflow_cleanup_generation = $5
                AND workflow_id = $6 AND positive_reset_required",
            &[
                &response_run_set_digest,
                &next_target_digest,
                &now_ms,
                &account_id,
                &generation,
                &lease.workflow_id,
            ],
        )? != 1
        {
            return Err(JobsWorkflowCommandError::InvalidState.into());
        }
        tx.execute(
            "UPDATE jobs_workflow_cleanup_targets
                SET lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND generation = $3 AND workflow_id = $4",
            &[&now_ms, &account_id, &generation, &lease.workflow_id],
        )?;
        tx.commit()?;
        return Ok(JobsWorkflowCleanupReceiptState::Pending);
    }
    if receipt.outcome == "pending" {
        let positive_presence = receipt.reason != "temporal_unavailable";
        let updated = if positive_presence {
            tx.execute(
                "UPDATE jobs_workflow_cleanup_v2_target_authorities
                    SET known_run_epoch = known_run_epoch + 1,
                        positive_reset_required = FALSE,
                        observation_pass = 1,
                        first_absence_observed_at_ms = NULL,
                        cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                        last_outcome_code = 'pending', lease_owner = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        next_attempt_at_ms = $1,
                        updated_at_ms = GREATEST($2, updated_at_ms + 1)
                  WHERE account_id = $3 AND workflow_cleanup_generation = $4
                    AND workflow_id = $5 AND positive_reset_required",
                &[
                    &(now_ms + WORKFLOW_CLEANUP_RETRY_MS),
                    &now_ms,
                    &account_id,
                    &generation,
                    &lease.workflow_id,
                ],
            )?
        } else {
            tx.execute(
                "UPDATE jobs_workflow_cleanup_v2_target_authorities
                    SET cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                        last_outcome_code = 'pending', lease_owner = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        next_attempt_at_ms = $1,
                        updated_at_ms = GREATEST($2, updated_at_ms + 1)
                  WHERE account_id = $3 AND workflow_cleanup_generation = $4
                    AND workflow_id = $5",
                &[
                    &(now_ms + WORKFLOW_CLEANUP_RETRY_MS),
                    &now_ms,
                    &account_id,
                    &generation,
                    &lease.workflow_id,
                ],
            )?
        };
        if updated != 1 {
            anyhow::bail!("workflow v2 proof epoch is not current")
        }
        tx.execute(
            "UPDATE jobs_workflow_cleanup_targets
                SET lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND generation = $3 AND workflow_id = $4",
            &[&now_ms, &account_id, &generation, &lease.workflow_id],
        )?;
        tx.commit()?;
        return Ok(JobsWorkflowCleanupReceiptState::Pending);
    }
    let subjects = if receipt.run_ids.is_empty() {
        vec![("workflow", "0".repeat(64))]
    } else {
        receipt
            .run_ids
            .iter()
            .map(|run_id| {
                Ok((
                    "run",
                    workflow_command_hmac(
                        "v2-cleanup-known-run-index-v3",
                        &json!({"runId": run_id, "workflowId": lease.workflow_id}),
                        1_024,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>>>()?
    };
    for (subject_kind, run_hmac) in subjects {
        tx.execute(
            "INSERT INTO jobs_workflow_cleanup_v2_run_observations (
                account_id, workflow_cleanup_generation, workflow_id,
                known_run_epoch, observation_pass, subject_kind,
                run_id_hmac_sha256, target_digest_sha256, request_epoch,
                cleanup_fence, cleanup_request_id, evidence_digest_sha256,
                describe_state, history_state, visibility_state, recorded_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                $12, 'not_found', 'not_found', 'not_found', $13)",
            &[
                &account_id,
                &generation,
                &lease.workflow_id,
                &known_run_epoch,
                &observation_pass,
                &subject_kind,
                &run_hmac,
                &lease.target_digest,
                &lease.request_epoch,
                &lease.cleanup_fence,
                &lease.cleanup_request_id,
                &receipt.evidence_digest,
                &now_ms,
            ],
        )?;
    }
    let result = if observation_pass == 1 {
        tx.execute(
            "UPDATE jobs_workflow_cleanup_v2_target_authorities
                SET observation_pass = 2, first_absence_observed_at_ms = $1,
                    cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = 'absence_observed', lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $1::BIGINT + $2::BIGINT,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $3 AND workflow_cleanup_generation = $4
                AND workflow_id = $5",
            &[
                &now_ms,
                &confirmation_age_ms,
                &account_id,
                &generation,
                &lease.workflow_id,
            ],
        )?;
        JobsWorkflowCleanupReceiptState::ObservationRecorded
    } else {
        tx.execute(
            "INSERT INTO jobs_workflow_execution_cleanup_observations (
                id, account_id, workflow_id, generation, target_set_hmac_sha256,
                observation_kind, observed_execution_run_id, cleanup_fence,
                cleanup_request_id, evidence_hmac_sha256, recorded_at_ms
             ) VALUES ($1, $2, $3, $4, $5, 'absence_proved', $6, $7, $8, $9, $10)",
            &[
                &format!("wfv2absence-v3-{}", uuid::Uuid::new_v4()),
                &account_id,
                &lease.workflow_id,
                &generation,
                &lease.target_set_digest,
                &lease.first_execution_run_id,
                &lease.cleanup_fence,
                &lease.cleanup_request_id,
                &receipt.evidence_digest,
                &now_ms,
            ],
        )?;
        tx.execute(
            "UPDATE jobs_workflow_cleanup_targets
                SET target_state = 'absence_proved', absence_proved_at_ms = $1,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND generation = $3 AND workflow_id = $4",
            &[&now_ms, &account_id, &generation, &lease.workflow_id],
        )?;
        tx.execute(
            "UPDATE jobs_workflow_executions
                SET lifecycle_state = 'absent', cleanup_state = 'absence_proved',
                    absence_proved_at_ms = $1, cleanup_lease_owner = NULL,
                    cleanup_lease_token_sha256 = NULL,
                    cleanup_lease_expires_at_ms = NULL,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND workflow_id = $3
                AND deletion_target_generation = $4",
            &[&now_ms, &account_id, &lease.workflow_id, &generation],
        )?;
        tx.execute(
            "UPDATE jobs_workflow_cleanup_v2_target_authorities
                SET cleanup_request_id = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = 'absence_observed', lease_owner = NULL,
                    lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                    next_attempt_at_ms = $1,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND workflow_cleanup_generation = $3
                AND workflow_id = $4",
            &[&now_ms, &account_id, &generation, &lease.workflow_id],
        )?;
        JobsWorkflowCleanupReceiptState::TargetComplete
    };
    if observation_pass == 1 {
        tx.execute(
            "UPDATE jobs_workflow_cleanup_targets
                SET lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL,
                    updated_at_ms = GREATEST($1, updated_at_ms + 1)
              WHERE account_id = $2 AND generation = $3 AND workflow_id = $4",
            &[&now_ms, &account_id, &generation, &lease.workflow_id],
        )?;
    }
    tx.commit()?;
    Ok(result)
}

fn initialize_v2_target_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    target: &FrozenWorkflowCleanupTarget,
    init: &V2TargetAuthorityInit<'_>,
) -> Result<()> {
    let known_run_ids = target
        .first_execution_run_id
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let set_digest = workflow_cleanup_v2_known_run_set_digest(&target.workflow_id, &known_run_ids)?;
    let target_digest = workflow_cleanup_v2_target_digest(
        init.cleanup_generation_id,
        init.target_set_digest,
        init.namespace,
        target,
    )?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_v2_target_authorities (
            account_id, workflow_cleanup_generation, workflow_id, known_run_epoch,
            known_run_set_digest_sha256, target_digest_sha256, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?6, ?6)",
        params![
            init.account_id,
            init.generation,
            target.workflow_id,
            set_digest,
            target_digest,
            init.now_ms
        ],
    )?;
    if let Some(run_id) = target.first_execution_run_id.as_deref() {
        let run_hmac = workflow_command_hmac(
            "v2-cleanup-known-run-index-v3",
            &json!({"runId": run_id, "workflowId": target.workflow_id}),
            1_024,
        )?;
        let run_digest = workflow_cleanup_sha256(
            WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
            &json!({"runId": run_id, "workflowId": target.workflow_id}),
            "workflow v2 known run identity",
        )?;
        tx.execute(
            "INSERT INTO jobs_workflow_cleanup_v2_known_runs (
                account_id, workflow_cleanup_generation, workflow_id,
                run_id_hmac_sha256, run_id_ciphertext, discovered_request_epoch,
                discovered_cleanup_fence, run_identity_digest_sha256, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)",
            params![
                init.account_id,
                init.generation,
                target.workflow_id,
                run_hmac,
                encrypt_payload(run_id)?,
                run_digest,
                init.now_ms,
            ],
        )?;
    }
    Ok(())
}

fn initialize_v2_target_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    target: &FrozenWorkflowCleanupTarget,
    init: &V2TargetAuthorityInit<'_>,
) -> Result<()> {
    let known_run_ids = target
        .first_execution_run_id
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let set_digest = workflow_cleanup_v2_known_run_set_digest(&target.workflow_id, &known_run_ids)?;
    let target_digest = workflow_cleanup_v2_target_digest(
        init.cleanup_generation_id,
        init.target_set_digest,
        init.namespace,
        target,
    )?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_v2_target_authorities (
            account_id, workflow_cleanup_generation, workflow_id, known_run_epoch,
            known_run_set_digest_sha256, target_digest_sha256, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, 1, $4, $5, $6, $6, $6)",
        &[
            &init.account_id,
            &init.generation,
            &target.workflow_id,
            &set_digest,
            &target_digest,
            &init.now_ms,
        ],
    )?;
    if let Some(run_id) = target.first_execution_run_id.as_deref() {
        let run_hmac = workflow_command_hmac(
            "v2-cleanup-known-run-index-v3",
            &json!({"runId": run_id, "workflowId": target.workflow_id}),
            1_024,
        )?;
        let run_digest = workflow_cleanup_sha256(
            WORKFLOW_CLEANUP_V2_KNOWN_RUN_SET_DOMAIN,
            &json!({"runId": run_id, "workflowId": target.workflow_id}),
            "workflow v2 known run identity",
        )?;
        tx.execute(
            "INSERT INTO jobs_workflow_cleanup_v2_known_runs (
                account_id, workflow_cleanup_generation, workflow_id,
                run_id_hmac_sha256, run_id_ciphertext, discovered_request_epoch,
                discovered_cleanup_fence, run_identity_digest_sha256, created_at_ms
             ) VALUES ($1, $2, $3, $4, $5, 0, 0, $6, $7)",
            &[
                &init.account_id,
                &init.generation,
                &target.workflow_id,
                &run_hmac,
                &encrypt_payload(run_id)?,
                &run_digest,
                &init.now_ms,
            ],
        )?;
    }
    Ok(())
}

fn freeze_jobs_workflow_cleanup_targets_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    account_generation: i64,
    now_ms: i64,
) -> Result<(i64, String)> {
    let generation = workflow_cleanup_account_generation(account_id, account_generation)?;
    cancel_never_delivered_workflow_commands_sqlite_tx(tx, account_id, now_ms)?;
    if let Some(existing) = tx
        .query_row(
            "SELECT generation, target_set_hmac_sha256, target_count,
                    legacy_reconciled, legacy_unresolved_count
               FROM jobs_workflow_cleanup_generations WHERE account_id = ?1",
            params![account_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
    {
        // Phase609 could have frozen a generation before the v3 companion
        // authority existed. Re-derive its immutable target set using the
        // storage generation actually persisted; the new opaque generation
        // is bound to that exact storage identity rather than rejecting it.
        let (target_set_digest, targets) =
            workflow_cleanup_targets_sqlite_tx(tx, account_id, existing.0, existing.3, existing.4)?;
        if target_set_digest != existing.1 || targets.len() as i64 != existing.2 {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        let cleanup_generation_id = workflow_cleanup_generation_id(
            account_id,
            account_generation,
            existing.0,
            &existing.1,
        )?;
        let namespace_ciphertext: String = tx.query_row(
            "SELECT legacy.namespace_ciphertext
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1",
            [],
            |row| row.get(0),
        )?;
        let namespace = decrypt_payload(&namespace_ciphertext)?;
        let target_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_workflow_cleanup_targets
              WHERE account_id = ?1 AND generation = ?2
                AND target_set_hmac_sha256 = ?3",
            params![account_id, existing.0, existing.1],
            |row| row.get(0),
        )?;
        if target_count != existing.2 {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        for target in &targets {
            let frozen = tx
                .query_row(
                    "SELECT start_command_id, start_request_id,
                            start_payload_hmac_sha256, first_execution_run_id,
                            target_state, managed_cloud_binding_sha256,
                            managed_cloud_release_memo_base64url,
                            managed_cloud_release_memo_sha256
                       FROM jobs_workflow_cleanup_targets
                      WHERE account_id = ?1 AND generation = ?2
                        AND target_set_hmac_sha256 = ?3 AND workflow_id = ?4",
                    params![account_id, existing.0, existing.1, target.workflow_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                        ))
                    },
                )
                .optional()?;
            let Some(frozen) = frozen else {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            };
            if frozen.0 != target.command_id
                || frozen.1 != target.request_id
                || frozen.2 != target.payload_hmac_sha256
                || frozen.3 != target.first_execution_run_id
                || frozen.5 != target.managed_cloud_binding_sha256
                || frozen.6 != target.managed_cloud_release_memo_base64url
                || frozen.7 != target.managed_cloud_release_memo_sha256
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let expected_state = if target.first_execution_run_id.is_some() {
                "cleanup_required"
            } else if target.command_state == "delivery_unknown" {
                "identity_reconcile"
            } else {
                "delivery_drain"
            };
            let pristine: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
                  WHERE target.account_id = ?1 AND target.generation = ?2
                    AND target.workflow_id = ?3 AND target.target_state = ?4
                    AND target.fence = 0 AND target.cleanup_request_id IS NULL
                    AND target.lease_owner IS NULL AND target.lease_token_sha256 IS NULL
                    AND target.lease_expires_at_ms IS NULL
                    AND target.absence_proved_at_ms IS NULL
                    AND NOT EXISTS (
                      SELECT 1 FROM jobs_workflow_execution_cleanup_observations observation
                       WHERE observation.account_id = target.account_id
                         AND observation.generation = target.generation
                         AND observation.workflow_id = target.workflow_id
                    )",
                params![account_id, existing.0, target.workflow_id, expected_state],
                |row| row.get(0),
            )?;
            if pristine != 1 || frozen.4 != expected_state {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if tx.query_row(
                "SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities
                  WHERE account_id = ?1 AND workflow_cleanup_generation = ?2
                    AND workflow_id = ?3",
                params![account_id, existing.0, target.workflow_id],
                |row| row.get::<_, i64>(0),
            )? != 0
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            initialize_v2_target_authority_sqlite_tx(
                tx,
                target,
                &V2TargetAuthorityInit {
                    account_id,
                    generation: existing.0,
                    cleanup_generation_id: &cleanup_generation_id,
                    target_set_digest: &existing.1,
                    namespace: &namespace,
                    now_ms,
                },
            )?;
        }
        let expected_bound = targets
            .iter()
            .filter(|target| target.first_execution_run_id.is_some())
            .count() as i64;
        let exactly_bound: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_workflow_executions
              WHERE account_id = ?1 AND first_execution_run_id IS NOT NULL
                AND deletion_target_generation = ?2
                AND deletion_target_hmac_sha256 = ?3",
            params![account_id, existing.0, existing.1],
            |row| row.get(0),
        )?;
        if exactly_bound != expected_bound {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return Ok((existing.0, existing.1));
    }
    let (target_set_digest, targets) =
        workflow_cleanup_targets_sqlite_tx(tx, account_id, generation, false, 1)?;
    let cleanup_generation_id = workflow_cleanup_generation_id(
        account_id,
        account_generation,
        generation,
        &target_set_digest,
    )?;
    let namespace_ciphertext: String = tx.query_row(
        "SELECT legacy.namespace_ciphertext
           FROM jobs_workflow_legacy_inventory_head head
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = head.generation
            AND legacy.query_digest_sha256 = head.query_digest_sha256
          WHERE head.singleton_id = 1",
        [],
        |row| row.get(0),
    )?;
    let namespace = decrypt_payload(&namespace_ciphertext)?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_generations (
            account_id, generation, state, target_set_hmac_sha256, target_count,
            legacy_reconciled, legacy_unresolved_count, frozen_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, 'frozen', ?3, ?4, 0, 1, ?5, ?5, ?5)",
        params![
            account_id,
            generation,
            target_set_digest,
            targets.len() as i64,
            now_ms,
        ],
    )?;
    for target in &targets {
        let target_state = if target.first_execution_run_id.is_some() {
            "cleanup_required"
        } else if target.command_state == "delivery_unknown" {
            "identity_reconcile"
        } else {
            "delivery_drain"
        };
        tx.execute(
            "INSERT INTO jobs_workflow_cleanup_targets (
                account_id, generation, target_set_hmac_sha256, workflow_id,
                start_command_id, start_request_id, start_payload_hmac_sha256,
                first_execution_run_id, managed_cloud_binding_sha256,
                managed_cloud_release_memo_base64url,
                managed_cloud_release_memo_sha256, target_state,
                created_at_ms, updated_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13
             )",
            params![
                account_id,
                generation,
                target_set_digest,
                target.workflow_id,
                target.command_id,
                target.request_id,
                target.payload_hmac_sha256,
                target.first_execution_run_id,
                target.managed_cloud_binding_sha256,
                target.managed_cloud_release_memo_base64url,
                target.managed_cloud_release_memo_sha256,
                target_state,
                now_ms,
            ],
        )?;
        initialize_v2_target_authority_sqlite_tx(
            tx,
            target,
            &V2TargetAuthorityInit {
                account_id,
                generation,
                cleanup_generation_id: &cleanup_generation_id,
                target_set_digest: &target_set_digest,
                namespace: &namespace,
                now_ms,
            },
        )?;
    }
    let bound = tx.execute(
        "UPDATE jobs_workflow_executions
            SET deletion_target_generation = ?1, deletion_target_hmac_sha256 = ?2,
                updated_at_ms = MAX(?3, updated_at_ms + 1)
          WHERE account_id = ?4 AND deletion_target_generation IS NULL",
        params![generation, target_set_digest, now_ms, account_id],
    )? as i64;
    let expected = targets
        .iter()
        .filter(|target| target.first_execution_run_id.is_some())
        .count() as i64;
    if bound != expected {
        anyhow::bail!("workflow cleanup target set changed while it was frozen")
    }
    Ok((generation, target_set_digest))
}

fn freeze_jobs_workflow_cleanup_targets_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    account_generation: i64,
    now_ms: i64,
) -> Result<(i64, String)> {
    let generation = workflow_cleanup_account_generation(account_id, account_generation)?;
    cancel_never_delivered_workflow_commands_postgres_tx(tx, account_id, now_ms)?;
    if let Some(row) = tx.query_opt(
        "SELECT generation, target_set_hmac_sha256, target_count,
                legacy_reconciled, legacy_unresolved_count
           FROM jobs_workflow_cleanup_generations
          WHERE account_id = $1 FOR UPDATE",
        &[&account_id],
    )? {
        let existing_generation: i64 = row.get(0);
        let existing_digest: String = row.get(1);
        let existing_target_count: i64 = row.get(2);
        let legacy_reconciled: bool = row.get(3);
        let legacy_unresolved_count: i64 = row.get(4);
        let (target_set_digest, targets) = workflow_cleanup_targets_postgres_tx(
            tx,
            account_id,
            existing_generation,
            legacy_reconciled,
            legacy_unresolved_count,
        )?;
        if target_set_digest != existing_digest || targets.len() as i64 != existing_target_count {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        let cleanup_generation_id = workflow_cleanup_generation_id(
            account_id,
            account_generation,
            existing_generation,
            &existing_digest,
        )?;
        let namespace_ciphertext: String = tx
            .query_one(
                "SELECT legacy.namespace_ciphertext
                   FROM jobs_workflow_legacy_inventory_head head
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = head.generation
                    AND legacy.query_digest_sha256 = head.query_digest_sha256
                  WHERE head.singleton_id = 1 FOR SHARE OF head, legacy",
                &[],
            )?
            .get(0);
        let namespace = decrypt_payload(&namespace_ciphertext)?;
        let actual_target_count: i64 = tx
            .query_one(
                "SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets
                  WHERE account_id = $1 AND generation = $2
                    AND target_set_hmac_sha256 = $3",
                &[&account_id, &existing_generation, &existing_digest],
            )?
            .get(0);
        if actual_target_count != existing_target_count {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        for target in &targets {
            let frozen = tx.query_opt(
                "SELECT start_command_id, start_request_id,
                        start_payload_hmac_sha256, first_execution_run_id,
                        target_state, managed_cloud_binding_sha256,
                        managed_cloud_release_memo_base64url,
                        managed_cloud_release_memo_sha256
                   FROM jobs_workflow_cleanup_targets
                  WHERE account_id = $1 AND generation = $2
                    AND target_set_hmac_sha256 = $3 AND workflow_id = $4
                  FOR SHARE",
                &[
                    &account_id,
                    &existing_generation,
                    &existing_digest,
                    &target.workflow_id,
                ],
            )?;
            let Some(frozen) = frozen else {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            };
            if frozen.get::<_, String>(0) != target.command_id
                || frozen.get::<_, String>(1) != target.request_id
                || frozen.get::<_, String>(2) != target.payload_hmac_sha256
                || frozen.get::<_, Option<String>>(3) != target.first_execution_run_id
                || frozen.get::<_, Option<String>>(5) != target.managed_cloud_binding_sha256
                || frozen.get::<_, Option<String>>(6) != target.managed_cloud_release_memo_base64url
                || frozen.get::<_, Option<String>>(7) != target.managed_cloud_release_memo_sha256
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let expected_state = if target.first_execution_run_id.is_some() {
                "cleanup_required"
            } else if target.command_state == "delivery_unknown" {
                "identity_reconcile"
            } else {
                "delivery_drain"
            };
            let pristine: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
                      WHERE target.account_id = $1 AND target.generation = $2
                        AND target.workflow_id = $3 AND target.target_state = $4
                        AND target.fence = 0 AND target.cleanup_request_id IS NULL
                        AND target.lease_owner IS NULL
                        AND target.lease_token_sha256 IS NULL
                        AND target.lease_expires_at_ms IS NULL
                        AND target.absence_proved_at_ms IS NULL
                        AND NOT EXISTS (
                          SELECT 1
                            FROM jobs_workflow_execution_cleanup_observations observation
                           WHERE observation.account_id = target.account_id
                             AND observation.generation = target.generation
                             AND observation.workflow_id = target.workflow_id
                        )",
                    &[
                        &account_id,
                        &existing_generation,
                        &target.workflow_id,
                        &expected_state,
                    ],
                )?
                .get(0);
            if pristine != 1 || frozen.get::<_, String>(4) != expected_state {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let authority_count: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint
                       FROM jobs_workflow_cleanup_v2_target_authorities
                      WHERE account_id = $1 AND workflow_cleanup_generation = $2
                        AND workflow_id = $3",
                    &[&account_id, &existing_generation, &target.workflow_id],
                )?
                .get(0);
            if authority_count != 0 {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            initialize_v2_target_authority_postgres_tx(
                tx,
                target,
                &V2TargetAuthorityInit {
                    account_id,
                    generation: existing_generation,
                    cleanup_generation_id: &cleanup_generation_id,
                    target_set_digest: &existing_digest,
                    namespace: &namespace,
                    now_ms,
                },
            )?;
        }
        let expected_bound = targets
            .iter()
            .filter(|target| target.first_execution_run_id.is_some())
            .count() as i64;
        let exactly_bound: i64 = tx
            .query_one(
                "SELECT COUNT(*)::bigint FROM jobs_workflow_executions
                  WHERE account_id = $1 AND first_execution_run_id IS NOT NULL
                    AND deletion_target_generation = $2
                    AND deletion_target_hmac_sha256 = $3",
                &[&account_id, &existing_generation, &existing_digest],
            )?
            .get(0);
        if exactly_bound != expected_bound {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return Ok((existing_generation, existing_digest));
    }
    let (target_set_digest, targets) =
        workflow_cleanup_targets_postgres_tx(tx, account_id, generation, false, 1)?;
    let cleanup_generation_id = workflow_cleanup_generation_id(
        account_id,
        account_generation,
        generation,
        &target_set_digest,
    )?;
    let namespace_ciphertext: String = tx
        .query_one(
            "SELECT legacy.namespace_ciphertext
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1 FOR SHARE OF head, legacy",
            &[],
        )?
        .get(0);
    let namespace = decrypt_payload(&namespace_ciphertext)?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_generations (
            account_id, generation, state, target_set_hmac_sha256, target_count,
            legacy_reconciled, legacy_unresolved_count, frozen_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1, $2, 'frozen', $3, $4, FALSE, 1, $5, $5, $5)",
        &[
            &account_id,
            &generation,
            &target_set_digest,
            &(targets.len() as i64),
            &now_ms,
        ],
    )?;
    for target in &targets {
        let target_state = if target.first_execution_run_id.is_some() {
            "cleanup_required"
        } else if target.command_state == "delivery_unknown" {
            "identity_reconcile"
        } else {
            "delivery_drain"
        };
        tx.execute(
            "INSERT INTO jobs_workflow_cleanup_targets (
                account_id, generation, target_set_hmac_sha256, workflow_id,
                start_command_id, start_request_id, start_payload_hmac_sha256,
                first_execution_run_id, managed_cloud_binding_sha256,
                managed_cloud_release_memo_base64url,
                managed_cloud_release_memo_sha256, target_state,
                created_at_ms, updated_at_ms
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13
             )",
            &[
                &account_id,
                &generation,
                &target_set_digest,
                &target.workflow_id,
                &target.command_id,
                &target.request_id,
                &target.payload_hmac_sha256,
                &target.first_execution_run_id,
                &target.managed_cloud_binding_sha256,
                &target.managed_cloud_release_memo_base64url,
                &target.managed_cloud_release_memo_sha256,
                &target_state,
                &now_ms,
            ],
        )?;
        initialize_v2_target_authority_postgres_tx(
            tx,
            target,
            &V2TargetAuthorityInit {
                account_id,
                generation,
                cleanup_generation_id: &cleanup_generation_id,
                target_set_digest: &target_set_digest,
                namespace: &namespace,
                now_ms,
            },
        )?;
    }
    let bound = tx.execute(
        "UPDATE jobs_workflow_executions
            SET deletion_target_generation = $1, deletion_target_hmac_sha256 = $2,
                updated_at_ms = GREATEST($3, updated_at_ms + 1)
          WHERE account_id = $4 AND deletion_target_generation IS NULL",
        &[&generation, &target_set_digest, &now_ms, &account_id],
    )? as i64;
    let expected = targets
        .iter()
        .filter(|target| target.first_execution_run_id.is_some())
        .count() as i64;
    if bound != expected {
        anyhow::bail!("workflow cleanup target set changed while it was frozen")
    }
    Ok((generation, target_set_digest))
}

fn require_fresh_legacy_generation_for_account_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    storage_generation: i64,
    query_digest: &str,
    now_ms: i64,
) -> Result<()> {
    let stored: (String, i64) = tx.query_row(
        "SELECT state, completion_epoch
           FROM jobs_workflow_legacy_inventory_generations
          WHERE generation = ?1 AND query_digest_sha256 = ?2",
        params![storage_generation, query_digest],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if stored.0 == "complete" {
        let next_epoch = stored
            .1
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let genesis = workflow_cleanup_genesis_digest(query_digest, next_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 1, page_index = 0,
                    predecessor_page_digest_sha256 = ?1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = NULL, next_attempt_at_ms = ?2,
                    completion_epoch = ?3, first_zero_observed_at_ms = NULL,
                    completed_at_ms = NULL, completion_digest_sha256 = NULL,
                    revalidate_after_ms = NULL, updated_at_ms = MAX(?2, updated_at_ms + 1)
              WHERE generation = ?4 AND state = 'complete' AND completion_epoch = ?5",
            params![genesis, now_ms, next_epoch, storage_generation, stored.1],
        )?;
    }
    Ok(())
}

fn require_fresh_legacy_generation_for_account_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    storage_generation: i64,
    query_digest: &str,
    now_ms: i64,
) -> Result<()> {
    let row = tx.query_one(
        "SELECT state, completion_epoch
           FROM jobs_workflow_legacy_inventory_generations
          WHERE generation = $1 AND query_digest_sha256 = $2 FOR UPDATE",
        &[&storage_generation, &query_digest],
    )?;
    let state: String = row.get(0);
    let completion_epoch: i64 = row.get(1);
    if state == "complete" {
        let next_epoch = completion_epoch
            .checked_add(1)
            .ok_or(JobsWorkflowCommandError::InvalidState)?;
        let genesis = workflow_cleanup_genesis_digest(query_digest, next_epoch)?;
        tx.execute(
            "UPDATE jobs_workflow_legacy_inventory_generations
                SET state = 'scanning', scan_pass = 1, page_index = 0,
                    predecessor_page_digest_sha256 = $1,
                    page_token_ciphertext = NULL, page_token_hmac_sha256 = NULL,
                    request_epoch = 0, fence = 0, request_id = NULL,
                    lease_owner = NULL, lease_token_sha256 = NULL,
                    lease_expires_at_ms = NULL, first_request_started_at_ms = NULL,
                    last_outcome_code = NULL, next_attempt_at_ms = $2,
                    completion_epoch = $3, first_zero_observed_at_ms = NULL,
                    completed_at_ms = NULL, completion_digest_sha256 = NULL,
                    revalidate_after_ms = NULL,
                    updated_at_ms = GREATEST($2, updated_at_ms + 1)
              WHERE generation = $4 AND state = 'complete' AND completion_epoch = $5",
            &[
                &genesis,
                &now_ms,
                &next_epoch,
                &storage_generation,
                &completion_epoch,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn freeze_jobs_workflow_cleanup_for_deletion_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    requested_at_ms: i64,
    legacy: &JobsLegacyInventoryAuthorityRef,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    validate_account_cleanup_freeze(account_id, requested_at_ms, legacy, caller_now_ms)?;
    let now_ms = sqlite_workflow_cleanup_db_now_ms(tx)?;
    let deletion_requested_at: i64 = tx.query_row(
        "SELECT requested_at_ms FROM account_deletion_intents WHERE account_id = ?1",
        params![account_id],
        |row| row.get(0),
    )?;
    if deletion_requested_at != requested_at_ms {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let current: (i64, String, String) = tx.query_row(
        "SELECT generation, inventory_generation_id, query_digest_sha256
           FROM jobs_workflow_legacy_inventory_head WHERE singleton_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if current.1 != legacy.inventory_generation_id
        || !workflow_command_hmac_matches(&current.2, &legacy.query_digest)
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    if let Some(binding) = tx
        .query_row(
            "SELECT account_generation, workflow_cleanup_generation,
                    target_set_hmac_sha256, legacy_generation,
                    legacy_inventory_generation_id, legacy_query_digest_sha256
               FROM jobs_workflow_cleanup_account_bindings WHERE account_id = ?1",
            params![account_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?
    {
        if binding.0 != requested_at_ms
            || binding.3 != current.0
            || binding.4 != legacy.inventory_generation_id
            || !workflow_command_hmac_matches(&binding.5, &legacy.query_digest)
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return workflow_cleanup_deletion_status_sqlite_tx(
            tx,
            account_id,
            requested_at_ms,
            now_ms,
            true,
        );
    }
    // A new account binding always advances a completed global proof to a
    // fresh database-owned epoch. The deletion request's process timestamp is
    // only an opaque generation key and never authorizes freshness.
    require_fresh_legacy_generation_for_account_sqlite_tx(tx, current.0, &current.2, now_ms)?;
    let (generation, target_set_digest) =
        freeze_jobs_workflow_cleanup_targets_sqlite_tx(tx, account_id, requested_at_ms, now_ms)?;
    let cleanup_generation_id = workflow_cleanup_generation_id(
        account_id,
        requested_at_ms,
        generation,
        &target_set_digest,
    )?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_account_bindings (
            account_id, account_generation, workflow_cleanup_generation,
            cleanup_generation_id, target_set_hmac_sha256, legacy_generation,
            legacy_inventory_generation_id, legacy_query_digest_sha256,
            state, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'frozen', ?9, ?9)",
        params![
            account_id,
            requested_at_ms,
            generation,
            cleanup_generation_id,
            target_set_digest,
            current.0,
            legacy.inventory_generation_id,
            legacy.query_digest,
            now_ms,
        ],
    )?;
    workflow_cleanup_deletion_status_sqlite_tx(tx, account_id, requested_at_ms, now_ms, false)
}

pub(crate) fn freeze_jobs_workflow_cleanup_for_deletion_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    requested_at_ms: i64,
    legacy: &JobsLegacyInventoryAuthorityRef,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    validate_account_cleanup_freeze(account_id, requested_at_ms, legacy, caller_now_ms)?;
    let now_ms = postgres_workflow_cleanup_db_now_ms(tx)?;
    let deletion_requested_at: i64 = tx
        .query_one(
            "SELECT requested_at_ms FROM account_deletion_intents
              WHERE account_id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .get(0);
    if deletion_requested_at != requested_at_ms {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let current = tx.query_one(
        "SELECT generation, inventory_generation_id, query_digest_sha256
           FROM jobs_workflow_legacy_inventory_head WHERE singleton_id = 1 FOR SHARE",
        &[],
    )?;
    if current.get::<_, String>(1) != legacy.inventory_generation_id
        || !workflow_command_hmac_matches(
            current.get::<_, String>(2).as_str(),
            &legacy.query_digest,
        )
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    if let Some(row) = tx.query_opt(
        "SELECT account_generation, workflow_cleanup_generation,
                target_set_hmac_sha256, legacy_generation,
                legacy_inventory_generation_id, legacy_query_digest_sha256
           FROM jobs_workflow_cleanup_account_bindings
          WHERE account_id = $1 FOR UPDATE",
        &[&account_id],
    )? {
        if row.get::<_, i64>(0) != requested_at_ms
            || row.get::<_, i64>(3) != current.get::<_, i64>(0)
            || row.get::<_, String>(4) != legacy.inventory_generation_id
            || !workflow_command_hmac_matches(
                row.get::<_, String>(5).as_str(),
                &legacy.query_digest,
            )
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return workflow_cleanup_deletion_status_postgres_tx(
            tx,
            account_id,
            requested_at_ms,
            now_ms,
            true,
        );
    }
    // A new account binding always advances a completed global proof to a
    // fresh database-owned epoch. The deletion request's process timestamp is
    // only an opaque generation key and never authorizes freshness.
    require_fresh_legacy_generation_for_account_postgres_tx(
        tx,
        current.get(0),
        current.get::<_, String>(2).as_str(),
        now_ms,
    )?;
    let (generation, target_set_digest) =
        freeze_jobs_workflow_cleanup_targets_postgres_tx(tx, account_id, requested_at_ms, now_ms)?;
    let cleanup_generation_id = workflow_cleanup_generation_id(
        account_id,
        requested_at_ms,
        generation,
        &target_set_digest,
    )?;
    tx.execute(
        "INSERT INTO jobs_workflow_cleanup_account_bindings (
            account_id, account_generation, workflow_cleanup_generation,
            cleanup_generation_id, target_set_hmac_sha256, legacy_generation,
            legacy_inventory_generation_id, legacy_query_digest_sha256,
            state, created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'frozen', $9, $9)",
        &[
            &account_id,
            &requested_at_ms,
            &generation,
            &cleanup_generation_id,
            &target_set_digest,
            &current.get::<_, i64>(0),
            &legacy.inventory_generation_id,
            &legacy.query_digest,
            &now_ms,
        ],
    )?;
    workflow_cleanup_deletion_status_postgres_tx(tx, account_id, requested_at_ms, now_ms, false)
}

fn workflow_cleanup_tombstone_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<JobsWorkflowCleanupCompletionTombstone> {
    Ok(JobsWorkflowCleanupCompletionTombstone {
        tombstone_id: row.get(0)?,
        account_generation: row.get(1)?,
        workflow_cleanup_generation: row.get(2)?,
        cleanup_generation_id: row.get(3)?,
        target_set_digest: row.get(4)?,
        legacy_generation: row.get(5)?,
        legacy_inventory_generation_id: row.get(6)?,
        legacy_completion_epoch: row.get(7)?,
        legacy_completion_digest: row.get(8)?,
        legacy_revalidate_after_ms: row.get(9)?,
        completion_digest: row.get(10)?,
        completed_at_ms: row.get(11)?,
    })
}

fn workflow_cleanup_tombstone_from_postgres_row(
    row: &postgres::Row,
) -> JobsWorkflowCleanupCompletionTombstone {
    JobsWorkflowCleanupCompletionTombstone {
        tombstone_id: row.get(0),
        account_generation: row.get(1),
        workflow_cleanup_generation: row.get(2),
        cleanup_generation_id: row.get(3),
        target_set_digest: row.get(4),
        legacy_generation: row.get(5),
        legacy_inventory_generation_id: row.get(6),
        legacy_completion_epoch: row.get(7),
        legacy_completion_digest: row.get(8),
        legacy_revalidate_after_ms: row.get(9),
        completion_digest: row.get(10),
        completed_at_ms: row.get(11),
    }
}

const WORKFLOW_CLEANUP_TOMBSTONE_SELECT: &str =
    "tombstone_id, account_generation, workflow_cleanup_generation,
     cleanup_generation_id, target_set_hmac_sha256, legacy_generation,
     legacy_inventory_generation_id, legacy_completion_epoch,
     legacy_completion_digest_sha256, legacy_revalidate_after_ms,
     completion_digest_sha256, completed_at_ms";

fn workflow_cleanup_deletion_status_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    account_generation: i64,
    now_ms: i64,
    replayed: bool,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    let binding = tx.query_row(
        "SELECT workflow_cleanup_generation, cleanup_generation_id,
                target_set_hmac_sha256, legacy_generation,
                legacy_inventory_generation_id, legacy_query_digest_sha256,
                state, completion_tombstone_id, completion_digest_sha256,
                object_sweep_started_at_ms, object_sweep_deleted_count,
                object_sweep_orphan_count
           FROM jobs_workflow_cleanup_account_bindings
          WHERE account_id = ?1 AND account_generation = ?2",
        params![account_id, account_generation],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        },
    )?;
    let v2_target_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_targets
          WHERE account_id = ?1 AND generation = ?2
            AND target_set_hmac_sha256 = ?3",
        params![account_id, binding.0, binding.2],
        |row| row.get(0),
    )?;
    let expected_v2_target_count: i64 = tx.query_row(
        "SELECT target_count FROM jobs_workflow_cleanup_generations
          WHERE account_id = ?1 AND generation = ?2
            AND target_set_hmac_sha256 = ?3",
        params![account_id, binding.0, binding.2],
        |row| row.get(0),
    )?;
    let v2_authority_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities
          WHERE account_id = ?1 AND workflow_cleanup_generation = ?2",
        params![account_id, binding.0],
        |row| row.get(0),
    )?;
    let v2_positive_reset_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities
          WHERE account_id = ?1 AND workflow_cleanup_generation = ?2
            AND positive_reset_required = 1",
        params![account_id, binding.0],
        |row| row.get(0),
    )?;
    let raw_v2_pending_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_targets
          WHERE account_id = ?1 AND generation = ?2
            AND target_set_hmac_sha256 = ?3 AND target_state <> 'absence_proved'",
        params![account_id, binding.0, binding.2],
        |row| row.get(0),
    )?;
    let v2_proved_target_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets
          WHERE account_id = ?1 AND generation = ?2
            AND target_set_hmac_sha256 = ?3",
        params![account_id, binding.0, binding.2],
        |row| row.get(0),
    )?;
    let v2_structure_incomplete = expected_v2_target_count != v2_target_count
        || expected_v2_target_count != v2_authority_count
        || v2_target_count - raw_v2_pending_count != v2_proved_target_count;
    let v2_pending_count =
        raw_v2_pending_count + v2_positive_reset_count + i64::from(v2_structure_incomplete);
    let legacy_pending_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_legacy_targets
          WHERE generation = ?1
            AND (target_state <> 'absence_proved' OR positive_reset_required = 1)",
        params![binding.3],
        |row| row.get(0),
    )?;
    let legacy_raw_authority_scrubbed: bool = tx.query_row(
        "SELECT legacy.page_token_ciphertext IS NULL
                AND legacy.page_token_hmac_sha256 IS NULL
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
                   WHERE page.generation = legacy.generation
                     AND page.raw_ciphertexts_scrubbed <> 1
                )
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_targets target
                   WHERE target.generation = legacy.generation
                     AND target.raw_ids_scrubbed <> 1
                )
           FROM jobs_workflow_legacy_inventory_generations legacy
          WHERE legacy.generation = ?1",
        params![binding.3],
        |row| row.get(0),
    )?;
    let legacy = tx
        .query_row(
            "SELECT legacy.state, legacy.completion_epoch,
                    legacy.completion_digest_sha256, legacy.revalidate_after_ms
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.inventory_generation_id = head.inventory_generation_id
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1 AND legacy.generation = ?1
                AND legacy.inventory_generation_id = ?2
                AND legacy.query_digest_sha256 = ?3",
            params![binding.3, binding.4, binding.5],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    let tombstone = match (binding.7.as_deref(), binding.8.as_deref()) {
        (Some(tombstone_id), Some(completion_digest)) => tx
            .query_row(
                &format!(
                    "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                       FROM jobs_workflow_cleanup_completion_tombstones
                      WHERE tombstone_id = ?1 AND completion_digest_sha256 = ?2"
                ),
                params![tombstone_id, completion_digest],
                workflow_cleanup_tombstone_from_sqlite_row,
            )
            .optional()?,
        (None, None) => None,
        _ => return Err(JobsWorkflowCommandError::InvalidState.into()),
    };
    let legacy_complete = legacy.as_ref().is_some_and(|value| {
        value.0 == "complete"
            && value.2.is_some()
            && value
                .3
                .is_some_and(|revalidate_after_ms| revalidate_after_ms > now_ms)
    }) && legacy_pending_count == 0
        && legacy_raw_authority_scrubbed;
    let complete = binding.6 == "complete"
        && v2_pending_count == 0
        && legacy_complete
        && tombstone.as_ref().is_some_and(|stored| {
            legacy.as_ref().is_some_and(|current| {
                stored.legacy_completion_epoch == current.1
                    && current.2.as_deref() == Some(stored.legacy_completion_digest.as_str())
                    && current.3 == Some(stored.legacy_revalidate_after_ms)
            }) && stored.account_generation == account_generation
                && stored.workflow_cleanup_generation == binding.0
                && stored.cleanup_generation_id == binding.1
                && stored.target_set_digest == binding.2
                && stored.legacy_generation == binding.3
                && stored.legacy_inventory_generation_id == binding.4
        });
    Ok(JobsWorkflowCleanupDeletionStatus {
        account_id: account_id.to_string(),
        account_generation,
        workflow_cleanup_generation: binding.0,
        cleanup_generation_id: binding.1,
        target_set_digest: binding.2,
        legacy_authority: JobsLegacyInventoryAuthorityRef {
            inventory_generation_id: binding.4,
            query_digest: binding.5,
        },
        v2_target_count,
        v2_pending_count,
        legacy_pending_count,
        legacy_complete,
        object_sweep_started_at_ms: binding.9,
        object_sweep_deleted_count: binding.10,
        object_sweep_orphan_count: binding.11,
        complete,
        tombstone,
        replayed,
    })
}

fn workflow_cleanup_deletion_status_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    account_generation: i64,
    now_ms: i64,
    replayed: bool,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    let binding = tx.query_one(
        "SELECT workflow_cleanup_generation, cleanup_generation_id,
                target_set_hmac_sha256, legacy_generation,
                legacy_inventory_generation_id, legacy_query_digest_sha256,
                state, completion_tombstone_id, completion_digest_sha256,
                object_sweep_started_at_ms, object_sweep_deleted_count,
                object_sweep_orphan_count
           FROM jobs_workflow_cleanup_account_bindings
          WHERE account_id = $1 AND account_generation = $2 FOR SHARE",
        &[&account_id, &account_generation],
    )?;
    let workflow_cleanup_generation: i64 = binding.get(0);
    let cleanup_generation_id: String = binding.get(1);
    let target_set_digest: String = binding.get(2);
    let legacy_generation: i64 = binding.get(3);
    let legacy_inventory_generation_id: String = binding.get(4);
    let legacy_query_digest: String = binding.get(5);
    let binding_state: String = binding.get(6);
    let tombstone_id: Option<String> = binding.get(7);
    let binding_completion_digest: Option<String> = binding.get(8);
    let object_sweep_started_at_ms: Option<i64> = binding.get(9);
    let object_sweep_deleted_count: i64 = binding.get(10);
    let object_sweep_orphan_count: i64 = binding.get(11);
    let generation = tx.query_one(
        "SELECT target_count FROM jobs_workflow_cleanup_generations
          WHERE account_id = $1 AND generation = $2
            AND target_set_hmac_sha256 = $3 FOR SHARE",
        &[
            &account_id,
            &workflow_cleanup_generation,
            &target_set_digest,
        ],
    )?;
    let expected_v2_target_count: i64 = generation.get(0);
    let counts = tx.query_one(
        "SELECT COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE target_state <> 'absence_proved')::bigint
           FROM jobs_workflow_cleanup_targets
          WHERE account_id = $1 AND generation = $2
            AND target_set_hmac_sha256 = $3",
        &[
            &account_id,
            &workflow_cleanup_generation,
            &target_set_digest,
        ],
    )?;
    let v2_target_count: i64 = counts.get(0);
    let raw_v2_pending_count: i64 = counts.get(1);
    let v2_authority_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint
               FROM jobs_workflow_cleanup_v2_target_authorities
              WHERE account_id = $1 AND workflow_cleanup_generation = $2",
            &[&account_id, &workflow_cleanup_generation],
        )?
        .get(0);
    let v2_positive_reset_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint
               FROM jobs_workflow_cleanup_v2_target_authorities
              WHERE account_id = $1 AND workflow_cleanup_generation = $2
                AND positive_reset_required",
            &[&account_id, &workflow_cleanup_generation],
        )?
        .get(0);
    let v2_proved_target_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint
               FROM jobs_workflow_cleanup_v2_proved_targets
              WHERE account_id = $1 AND generation = $2
                AND target_set_hmac_sha256 = $3",
            &[
                &account_id,
                &workflow_cleanup_generation,
                &target_set_digest,
            ],
        )?
        .get(0);
    let v2_structure_incomplete = expected_v2_target_count != v2_target_count
        || expected_v2_target_count != v2_authority_count
        || v2_target_count - raw_v2_pending_count != v2_proved_target_count;
    let v2_pending_count =
        raw_v2_pending_count + v2_positive_reset_count + i64::from(v2_structure_incomplete);
    let legacy_pending_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_workflow_legacy_targets
              WHERE generation = $1
                AND (target_state <> 'absence_proved' OR positive_reset_required)",
            &[&legacy_generation],
        )?
        .get(0);
    let legacy_raw_authority_scrubbed: bool = tx
        .query_one(
            "SELECT legacy.page_token_ciphertext IS NULL
                    AND legacy.page_token_hmac_sha256 IS NULL
                    AND NOT EXISTS (
                      SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
                       WHERE page.generation = legacy.generation
                         AND NOT page.raw_ciphertexts_scrubbed
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM jobs_workflow_legacy_targets target
                       WHERE target.generation = legacy.generation
                         AND NOT target.raw_ids_scrubbed
                    )
               FROM jobs_workflow_legacy_inventory_generations legacy
              WHERE legacy.generation = $1",
            &[&legacy_generation],
        )?
        .get(0);
    let legacy = tx.query_opt(
        "SELECT legacy.state, legacy.completion_epoch,
                legacy.completion_digest_sha256, legacy.revalidate_after_ms
           FROM jobs_workflow_legacy_inventory_head head
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = head.generation
            AND legacy.inventory_generation_id = head.inventory_generation_id
            AND legacy.query_digest_sha256 = head.query_digest_sha256
          WHERE head.singleton_id = 1 AND legacy.generation = $1
            AND legacy.inventory_generation_id = $2
            AND legacy.query_digest_sha256 = $3 FOR SHARE OF head, legacy",
        &[
            &legacy_generation,
            &legacy_inventory_generation_id,
            &legacy_query_digest,
        ],
    )?;
    let tombstone = match (
        tombstone_id.as_deref(),
        binding_completion_digest.as_deref(),
    ) {
        (Some(tombstone_id), Some(completion_digest)) => tx
            .query_opt(
                &format!(
                    "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                       FROM jobs_workflow_cleanup_completion_tombstones
                      WHERE tombstone_id = $1 AND completion_digest_sha256 = $2"
                ),
                &[&tombstone_id, &completion_digest],
            )?
            .as_ref()
            .map(workflow_cleanup_tombstone_from_postgres_row),
        (None, None) => None,
        _ => return Err(JobsWorkflowCommandError::InvalidState.into()),
    };
    let legacy_complete = legacy.as_ref().is_some_and(|row| {
        row.get::<_, String>(0) == "complete"
            && row.get::<_, Option<String>>(2).is_some()
            && row
                .get::<_, Option<i64>>(3)
                .is_some_and(|revalidate_after_ms| revalidate_after_ms > now_ms)
            && legacy_pending_count == 0
            && legacy_raw_authority_scrubbed
    });
    let complete = binding_state == "complete"
        && v2_pending_count == 0
        && legacy_complete
        && tombstone.as_ref().is_some_and(|stored| {
            legacy.as_ref().is_some_and(|row| {
                stored.legacy_completion_epoch == row.get::<_, i64>(1)
                    && row.get::<_, Option<String>>(2).as_deref()
                        == Some(stored.legacy_completion_digest.as_str())
                    && row.get::<_, Option<i64>>(3) == Some(stored.legacy_revalidate_after_ms)
            }) && stored.account_generation == account_generation
                && stored.workflow_cleanup_generation == workflow_cleanup_generation
                && stored.cleanup_generation_id == cleanup_generation_id
                && stored.target_set_digest == target_set_digest
                && stored.legacy_generation == legacy_generation
                && stored.legacy_inventory_generation_id == legacy_inventory_generation_id
        });
    Ok(JobsWorkflowCleanupDeletionStatus {
        account_id: account_id.to_string(),
        account_generation,
        workflow_cleanup_generation,
        cleanup_generation_id,
        target_set_digest,
        legacy_authority: JobsLegacyInventoryAuthorityRef {
            inventory_generation_id: legacy_inventory_generation_id,
            query_digest: legacy_query_digest,
        },
        v2_target_count,
        v2_pending_count,
        legacy_pending_count,
        legacy_complete,
        object_sweep_started_at_ms,
        object_sweep_deleted_count,
        object_sweep_orphan_count,
        complete,
        tombstone,
        replayed,
    })
}

pub fn get_jobs_workflow_cleanup_deletion_status(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
) -> Result<Option<JobsWorkflowCleanupDeletionStatus>> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let status_now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let exists: bool = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                     WHERE account_id = ?1 AND account_generation = ?2
                 )",
                params![account_id, requested_at_ms],
                |row| row.get(0),
            )?;
            if !exists {
                tx.commit()?;
                return Ok(None);
            }
            let status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                status_now_ms,
                true,
            )?;
            tx.commit()?;
            Ok(Some(status))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let status_now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            let exists: bool = tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                         WHERE account_id = $1 AND account_generation = $2
                     )",
                    &[&account_id, &requested_at_ms],
                )?
                .get(0);
            if !exists {
                tx.commit()?;
                return Ok(None);
            }
            let status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                status_now_ms,
                true,
            )?;
            tx.commit()?;
            Ok(Some(status))
        }
    })
}

fn workflow_cleanup_completion_material(
    status: &JobsWorkflowCleanupDeletionStatus,
    legacy_completion_epoch: i64,
    legacy_completion_digest: &str,
    legacy_revalidate_after_ms: i64,
    now_ms: i64,
) -> Result<(String, String, String)> {
    let account_subject_hmac = workflow_command_hmac(
        "workflow-cleanup-account-subject-v3",
        &json!({
            "accountGeneration": status.account_generation,
            "accountId": status.account_id,
        }),
        1_024,
    )?;
    let completion_digest = workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_COMPLETION_DIGEST_DOMAIN,
        &json!({
            "accountGeneration": status.account_generation,
            "accountSubjectDigest": account_subject_hmac,
            "cleanupGenerationId": status.cleanup_generation_id,
            "inventoryGenerationId": status.legacy_authority.inventory_generation_id,
            "legacyCompletionDigest": legacy_completion_digest,
            "legacyCompletionEpoch": legacy_completion_epoch,
            "legacyRevalidateAfterMs": legacy_revalidate_after_ms,
            "queryDigest": status.legacy_authority.query_digest,
            "targetSetDigest": status.target_set_digest,
            "v2TargetCount": status.v2_target_count,
        }),
        "workflow cleanup completion",
    )?;
    let tombstone_id = format!("wfcleantomb-v3-{}", &completion_digest[..32]);
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok((account_subject_hmac, completion_digest, tombstone_id))
}

pub fn verify_jobs_workflow_cleanup_deletion_ready(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&caller_now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let mut status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            if status.complete {
                tx.commit()?;
                return Ok(status);
            }
            if status.v2_pending_count != 0
                || status.legacy_pending_count != 0
                || !status.legacy_complete
            {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET state = 'draining', completion_tombstone_id = NULL,
                            completion_digest_sha256 = NULL,
                            hard_delete_authorized_at_ms = NULL,
                            hard_delete_sweep_attempt_id = NULL,
                            hard_delete_authorization_digest_sha256 = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND account_generation = ?3
                        AND state <> 'draining'",
                    params![now_ms, account_id, requested_at_ms],
                )?;
                status.complete = false;
                status.tombstone = None;
                tx.commit()?;
                return Ok(status);
            }
            let (
                legacy_storage_generation,
                legacy_epoch,
                legacy_digest,
                legacy_revalidate_after_ms,
            ): (i64, i64, String, i64) = tx.query_row(
                "SELECT legacy.generation, legacy.completion_epoch,
                        legacy.completion_digest_sha256, legacy.revalidate_after_ms
                   FROM jobs_workflow_legacy_inventory_head head
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = head.generation
                    AND legacy.inventory_generation_id = head.inventory_generation_id
                    AND legacy.query_digest_sha256 = head.query_digest_sha256
                  WHERE head.singleton_id = 1 AND legacy.state = 'complete'
                    AND legacy.inventory_generation_id = ?1
                    AND legacy.query_digest_sha256 = ?2",
                params![
                    status.legacy_authority.inventory_generation_id,
                    status.legacy_authority.query_digest
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
            let (subject_hmac, completion_digest, tombstone_id) =
                workflow_cleanup_completion_material(
                    &status,
                    legacy_epoch,
                    &legacy_digest,
                    legacy_revalidate_after_ms,
                    now_ms,
                )?;
            tx.execute(
                "INSERT INTO jobs_workflow_cleanup_completion_tombstones (
                    tombstone_id, account_subject_hmac_sha256, account_generation,
                    workflow_cleanup_generation, cleanup_generation_id,
                    target_set_hmac_sha256, legacy_generation,
                    legacy_inventory_generation_id, legacy_completion_epoch,
                    legacy_completion_digest_sha256, legacy_revalidate_after_ms,
                    completion_digest_sha256, completed_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(tombstone_id) DO NOTHING",
                params![
                    tombstone_id,
                    subject_hmac,
                    status.account_generation,
                    status.workflow_cleanup_generation,
                    status.cleanup_generation_id,
                    status.target_set_digest,
                    legacy_storage_generation,
                    status.legacy_authority.inventory_generation_id,
                    legacy_epoch,
                    legacy_digest,
                    legacy_revalidate_after_ms,
                    completion_digest,
                    now_ms,
                ],
            )?;
            let stored = tx.query_row(
                &format!(
                    "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                       FROM jobs_workflow_cleanup_completion_tombstones
                      WHERE tombstone_id = ?1"
                ),
                params![tombstone_id],
                workflow_cleanup_tombstone_from_sqlite_row,
            )?;
            if stored.completion_digest != completion_digest
                || stored.account_generation != status.account_generation
                || stored.cleanup_generation_id != status.cleanup_generation_id
                || stored.legacy_inventory_generation_id
                    != status.legacy_authority.inventory_generation_id
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if tx.execute(
                "UPDATE jobs_workflow_cleanup_account_bindings
                    SET state = 'complete', completion_tombstone_id = ?1,
                        completion_digest_sha256 = ?2,
                        hard_delete_authorized_at_ms = NULL,
                        hard_delete_sweep_attempt_id = NULL,
                        hard_delete_authorization_digest_sha256 = NULL,
                        updated_at_ms = MAX(?3, updated_at_ms + 1)
                  WHERE account_id = ?4 AND account_generation = ?5
                    AND cleanup_generation_id = ?6
                    AND legacy_inventory_generation_id = ?7
                    AND legacy_query_digest_sha256 = ?8",
                params![
                    tombstone_id,
                    completion_digest,
                    now_ms,
                    account_id,
                    requested_at_ms,
                    status.cleanup_generation_id,
                    status.legacy_authority.inventory_generation_id,
                    status.legacy_authority.query_digest,
                ],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let result = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                false,
            )?;
            if !result.complete {
                return Err(JobsWorkflowCommandError::InvalidState.into());
            }
            tx.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                      WHERE account_id = $1 AND account_generation = $2 FOR UPDATE",
                    &[&account_id, &requested_at_ms],
                )?
                .is_none()
            {
                return Err(JobsWorkflowCommandError::InvalidState.into());
            }
            let mut status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            if status.complete {
                tx.commit()?;
                return Ok(status);
            }
            if status.v2_pending_count != 0
                || status.legacy_pending_count != 0
                || !status.legacy_complete
            {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET state = 'draining', completion_tombstone_id = NULL,
                            completion_digest_sha256 = NULL,
                            hard_delete_authorized_at_ms = NULL,
                            hard_delete_sweep_attempt_id = NULL,
                            hard_delete_authorization_digest_sha256 = NULL,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE account_id = $2 AND account_generation = $3
                        AND state <> 'draining'",
                    &[&now_ms, &account_id, &requested_at_ms],
                )?;
                status.complete = false;
                status.tombstone = None;
                tx.commit()?;
                return Ok(status);
            }
            let legacy = tx.query_one(
                "SELECT legacy.generation, legacy.completion_epoch,
                        legacy.completion_digest_sha256, legacy.revalidate_after_ms
                   FROM jobs_workflow_legacy_inventory_head head
                   JOIN jobs_workflow_legacy_inventory_generations legacy
                     ON legacy.generation = head.generation
                    AND legacy.inventory_generation_id = head.inventory_generation_id
                    AND legacy.query_digest_sha256 = head.query_digest_sha256
                  WHERE head.singleton_id = 1 AND legacy.state = 'complete'
                    AND legacy.inventory_generation_id = $1
                    AND legacy.query_digest_sha256 = $2 FOR UPDATE OF head, legacy",
                &[
                    &status.legacy_authority.inventory_generation_id,
                    &status.legacy_authority.query_digest,
                ],
            )?;
            let legacy_storage_generation: i64 = legacy.get(0);
            let legacy_epoch: i64 = legacy.get(1);
            let legacy_digest: String = legacy.get(2);
            let legacy_revalidate_after_ms: i64 = legacy.get(3);
            let (subject_hmac, completion_digest, tombstone_id) =
                workflow_cleanup_completion_material(
                    &status,
                    legacy_epoch,
                    &legacy_digest,
                    legacy_revalidate_after_ms,
                    now_ms,
                )?;
            tx.execute(
                "INSERT INTO jobs_workflow_cleanup_completion_tombstones (
                    tombstone_id, account_subject_hmac_sha256, account_generation,
                    workflow_cleanup_generation, cleanup_generation_id,
                    target_set_hmac_sha256, legacy_generation,
                    legacy_inventory_generation_id, legacy_completion_epoch,
                    legacy_completion_digest_sha256, legacy_revalidate_after_ms,
                    completion_digest_sha256, completed_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(tombstone_id) DO NOTHING",
                &[
                    &tombstone_id,
                    &subject_hmac,
                    &status.account_generation,
                    &status.workflow_cleanup_generation,
                    &status.cleanup_generation_id,
                    &status.target_set_digest,
                    &legacy_storage_generation,
                    &status.legacy_authority.inventory_generation_id,
                    &legacy_epoch,
                    &legacy_digest,
                    &legacy_revalidate_after_ms,
                    &completion_digest,
                    &now_ms,
                ],
            )?;
            let stored = workflow_cleanup_tombstone_from_postgres_row(&tx.query_one(
                &format!(
                    "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                       FROM jobs_workflow_cleanup_completion_tombstones
                      WHERE tombstone_id = $1 FOR SHARE"
                ),
                &[&tombstone_id],
            )?);
            if stored.completion_digest != completion_digest
                || stored.account_generation != status.account_generation
                || stored.cleanup_generation_id != status.cleanup_generation_id
                || stored.legacy_inventory_generation_id
                    != status.legacy_authority.inventory_generation_id
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if tx.execute(
                "UPDATE jobs_workflow_cleanup_account_bindings
                    SET state = 'complete', completion_tombstone_id = $1,
                        completion_digest_sha256 = $2,
                        hard_delete_authorized_at_ms = NULL,
                        hard_delete_sweep_attempt_id = NULL,
                        hard_delete_authorization_digest_sha256 = NULL,
                        updated_at_ms = GREATEST($3, updated_at_ms + 1)
                  WHERE account_id = $4 AND account_generation = $5
                    AND cleanup_generation_id = $6
                    AND legacy_inventory_generation_id = $7
                    AND legacy_query_digest_sha256 = $8",
                &[
                    &tombstone_id,
                    &completion_digest,
                    &now_ms,
                    &account_id,
                    &requested_at_ms,
                    &status.cleanup_generation_id,
                    &status.legacy_authority.inventory_generation_id,
                    &status.legacy_authority.query_digest,
                ],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let result = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                false,
            )?;
            if !result.complete {
                return Err(JobsWorkflowCommandError::InvalidState.into());
            }
            tx.commit()?;
            Ok(result)
        }
    })
}

pub fn get_jobs_workflow_cleanup_tombstone(
    pool: &DbPool,
    tombstone_id: &str,
) -> Result<Option<JobsWorkflowCleanupCompletionTombstone>> {
    if !workflow_command_opaque_identifier(tombstone_id, 128) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let connection = pool.get()?;
            Ok(connection
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                           FROM jobs_workflow_cleanup_completion_tombstones
                          WHERE tombstone_id = ?1"
                    ),
                    params![tombstone_id],
                    workflow_cleanup_tombstone_from_sqlite_row,
                )
                .optional()?)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            Ok(connection
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_CLEANUP_TOMBSTONE_SELECT}
                           FROM jobs_workflow_cleanup_completion_tombstones
                          WHERE tombstone_id = $1"
                    ),
                    &[&tombstone_id],
                )?
                .as_ref()
                .map(workflow_cleanup_tombstone_from_postgres_row))
        }
    })
}

fn prepare_jobs_workflow_cleanup_object_sweep(
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    manifests: &[JobsWorkflowCleanupObjectSweepScopeManifest],
    proof: &JobsWorkflowCleanupDeletionProof,
) -> Result<(Vec<PreparedObjectSweepScope>, String, i64)> {
    validate_jobs_workflow_cleanup_deletion_proof(account_id, requested_at_ms, proof)?;
    if !workflow_command_opaque_identifier(sweep_attempt_id, 128)
        || manifests.is_empty()
        || manifests.len() > WORKFLOW_CLEANUP_MAX_SWEEP_SCOPES
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let mut prepared = Vec::with_capacity(manifests.len());
    let mut previous_scope: Option<&str> = None;
    let mut known_object_count = 0_i64;
    for manifest in manifests {
        if !workflow_command_opaque_identifier(&manifest.scope_id, 128)
            || !manifest.prefix_sweep
            || previous_scope.is_some_and(|previous| previous >= manifest.scope_id.as_str())
        {
            return Err(JobsWorkflowCommandError::InvalidRequest.into());
        }
        previous_scope = Some(&manifest.scope_id);
        let mut previous_key: Option<&str> = None;
        let mut object_key_hmacs = Vec::with_capacity(manifest.object_keys.len());
        for object_key in &manifest.object_keys {
            if object_key.is_empty()
                || object_key.len() > 2_048
                || previous_key.is_some_and(|previous| previous >= object_key.as_str())
            {
                return Err(JobsWorkflowCommandError::InvalidRequest.into());
            }
            previous_key = Some(object_key);
            object_key_hmacs.push(workflow_command_hmac(
                "workflow-cleanup-object-sweep-key-v3",
                &json!({
                    "accountGeneration": requested_at_ms,
                    "accountId": account_id,
                    "objectKey": object_key,
                    "scopeId": manifest.scope_id,
                }),
                4_096,
            )?);
        }
        let object_count = i64::try_from(object_key_hmacs.len())
            .map_err(|_| JobsWorkflowCommandError::InvalidRequest)?;
        known_object_count = known_object_count
            .checked_add(object_count)
            .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
        let manifest_digest = workflow_cleanup_sha256(
            WORKFLOW_CLEANUP_SWEEP_MANIFEST_DIGEST_DOMAIN,
            &json!({
                "objectKeyDigests": object_key_hmacs,
                "prefixSweep": manifest.prefix_sweep,
                "scopeId": manifest.scope_id,
            }),
            "workflow cleanup object sweep manifest",
        )?;
        prepared.push(PreparedObjectSweepScope {
            scope_id: manifest.scope_id.clone(),
            object_key_hmacs,
            prefix_sweep: manifest.prefix_sweep,
            manifest_digest,
        });
    }
    let scope_material = prepared
        .iter()
        .map(|scope| {
            json!({
                "manifestDigest": scope.manifest_digest,
                "objectCount": scope.object_key_hmacs.len(),
                "prefixSweep": scope.prefix_sweep,
                "scopeId": scope.scope_id,
            })
        })
        .collect::<Vec<_>>();
    let scope_set_digest = workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_SWEEP_SCOPE_SET_DIGEST_DOMAIN,
        &json!({"scopes": scope_material}),
        "workflow cleanup object sweep scope set",
    )?;
    Ok((prepared, scope_set_digest, known_object_count))
}

fn workflow_cleanup_sweep_authorization_digest(
    material: &SweepAuthorizationDigestMaterial<'_>,
) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_SWEEP_AUTHORIZATION_DIGEST_DOMAIN,
        &json!({
            "accountGeneration": material.requested_at_ms,
            "accountId": material.account_id,
            "cleanupGenerationId": material.proof.cleanup_generation_id,
            "completionDigest": material.proof.completion_digest,
            "inventoryGenerationId": material.proof.legacy_authority.inventory_generation_id,
            "knownObjectCount": material.known_object_count,
            "queryDigest": material.proof.legacy_authority.query_digest,
            "runnerAuthorityDigest": material.runner.authority_digest,
            "runnerPurgeRequestId": material.runner.purge_request_id,
            "scopeCount": material.scope_count,
            "scopeSetDigest": material.scope_set_digest,
            "sweepAttemptId": material.sweep_attempt_id,
            "targetSetDigest": material.proof.target_set_digest,
            "tombstoneId": material.proof.tombstone_id,
        }),
        "workflow cleanup object sweep authorization",
    )
}

fn workflow_cleanup_runner_sweep_authority_digest(
    material: &RunnerSweepAuthorityMaterial,
) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_SWEEP_RUNNER_AUTHORITY_DIGEST_DOMAIN,
        &json!({
            "completedAtMs": material.completed_at_ms,
            "legacyInventoryAuthorityId": material.legacy_authority_id,
            "legacyInventoryAuthoritySha256": material.legacy_authority_sha256,
            "legacyInventoryGeneration": material.legacy_inventory_generation,
            "legacyInventoryReconciliationId": material.legacy_reconciliation_id,
            "purgeGeneration": material.purge_generation,
            "purgeSubject": material.purge_subject,
            "requestId": material.purge_request_id,
            "requiredTargetCount": material.required_target_count,
            "targetSetDigest": material.target_set_digest,
            "tombstoneGeneration": material.tombstone_generation,
        }),
        "workflow cleanup runner sweep authority",
    )
}

// This public cross-slice contract deliberately keeps each independently
// replay-bound authority component explicit for account-deletion callers.
#[allow(clippy::too_many_arguments)]
pub fn begin_jobs_workflow_cleanup_object_sweep(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    runner_purge_request_id: &str,
    sweep_attempt_id: &str,
    manifests: &[JobsWorkflowCleanupObjectSweepScopeManifest],
    proof: &JobsWorkflowCleanupDeletionProof,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupObjectSweepBeginResult> {
    if !workflow_command_opaque_identifier(runner_purge_request_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&caller_now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let (prepared, scope_set_digest, known_object_count) =
        prepare_jobs_workflow_cleanup_object_sweep(
            account_id,
            requested_at_ms,
            sweep_attempt_id,
            manifests,
            proof,
        )?;
    let scope_count =
        i64::try_from(prepared.len()).map_err(|_| JobsWorkflowCommandError::InvalidRequest)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // SQLite IMMEDIATE serializes the runner-fleet/account/workflow
            // authority recheck in the same logical order as PostgreSQL.
            let _: i64 = tx.query_row(
                "SELECT singleton_id FROM jobs_runner_volume_fleet_state
                  WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            let _: i64 = tx.query_row(
                "SELECT account_generation
                   FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = ?1 AND account_generation = ?2",
                params![account_id, requested_at_ms],
                |row| row.get(0),
            )?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let mut status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            if status.deletion_proof().as_ref() != Some(proof) {
                tx.commit()?;
                return Ok(JobsWorkflowCleanupObjectSweepBeginResult::PendingWorkflow(
                    status,
                ));
            }
            let runner = tx
                .query_row(
                    "SELECT request.purge_generation, tombstone.tombstone_generation,
                            request.legacy_inventory_generation,
                            request.legacy_inventory_reconciliation_id,
                            request.legacy_inventory_authority_id,
                            request.legacy_inventory_authority_sha256,
                            request.purge_subject, request.target_set_sha256,
                            request.required_target_count, request.completed_at_ms
                       FROM jobs_runner_volume_fleet_state fleet
                       JOIN jobs_runner_purge_requests request
                         ON request.legacy_inventory_generation =
                            fleet.legacy_inventory_generation
                        AND request.legacy_inventory_reconciliation_id =
                            fleet.legacy_inventory_reconciliation_id
                        AND request.legacy_inventory_authority_id =
                            fleet.legacy_inventory_authority_id
                        AND request.legacy_inventory_authority_sha256 =
                            fleet.legacy_inventory_authority_sha256
                       JOIN jobs_runner_purge_tombstones tombstone
                         ON tombstone.request_id = request.request_id
                        AND tombstone.purge_generation = request.purge_generation
                        AND tombstone.purge_subject = request.purge_subject
                        AND tombstone.target_set_sha256 = request.target_set_sha256
                        AND tombstone.required_target_count = request.required_target_count
                        AND tombstone.completed_at_ms = request.completed_at_ms
                      WHERE fleet.singleton_id = 1
                        AND fleet.legacy_inventory_state = 'ready'
                        AND request.request_id = ?1 AND request.account_id = ?2
                        AND request.state = 'complete'
                        AND request.legacy_unresolved_count = 0
                        AND request.resolved_target_count = request.required_target_count",
                    params![runner_purge_request_id, account_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some(runner) = runner else {
                tx.commit()?;
                return Ok(JobsWorkflowCleanupObjectSweepBeginResult::PendingRunner(
                    status,
                ));
            };
            let runner_material = RunnerSweepAuthorityMaterial {
                purge_request_id: runner_purge_request_id.to_owned(),
                purge_subject: runner.6,
                purge_generation: runner.0,
                tombstone_generation: runner.1,
                target_set_digest: runner.7,
                required_target_count: runner.8,
                completed_at_ms: runner.9,
                legacy_inventory_generation: runner.2,
                legacy_reconciliation_id: runner.3,
                legacy_authority_id: runner.4,
                legacy_authority_sha256: runner.5,
            };
            let runner_authority = StoredRunnerSweepAuthority {
                purge_request_id: runner_material.purge_request_id.clone(),
                purge_generation: runner_material.purge_generation,
                tombstone_generation: runner_material.tombstone_generation,
                legacy_inventory_generation: runner_material.legacy_inventory_generation,
                legacy_reconciliation_id: runner_material.legacy_reconciliation_id.clone(),
                legacy_authority_id: runner_material.legacy_authority_id.clone(),
                legacy_authority_sha256: runner_material.legacy_authority_sha256.clone(),
                authority_digest: workflow_cleanup_runner_sweep_authority_digest(&runner_material)?,
            };
            let authorization_digest =
                workflow_cleanup_sweep_authorization_digest(&SweepAuthorizationDigestMaterial {
                    account_id,
                    requested_at_ms,
                    sweep_attempt_id,
                    scope_count: prepared.len(),
                    known_object_count,
                    scope_set_digest: &scope_set_digest,
                    runner: &runner_authority,
                    proof,
                })?;
            let existing = tx
                .query_row(
                    "SELECT completion_tombstone_id, completion_digest_sha256,
                            authorization_digest_sha256, scope_set_digest_sha256,
                            scope_count, known_object_count, sealed,
                            runner_purge_request_id, runner_purge_generation,
                            runner_purge_tombstone_generation,
                            runner_legacy_inventory_generation,
                            runner_legacy_reconciliation_id,
                            runner_legacy_authority_id,
                            runner_legacy_authority_sha256,
                            runner_authority_digest_sha256
                       FROM jobs_workflow_cleanup_object_sweep_authorizations
                      WHERE account_id = ?1 AND account_generation = ?2
                        AND sweep_attempt_id = ?3",
                    params![account_id, requested_at_ms, sweep_attempt_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, i64>(9)?,
                            row.get::<_, i64>(10)?,
                            row.get::<_, String>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, String>(13)?,
                            row.get::<_, String>(14)?,
                        ))
                    },
                )
                .optional()?;
            let replayed = if let Some(stored) = existing {
                if stored.0 != proof.tombstone_id
                    || !workflow_command_hmac_matches(&stored.1, &proof.completion_digest)
                    || !workflow_command_hmac_matches(&stored.2, &authorization_digest)
                    || !workflow_command_hmac_matches(&stored.3, &scope_set_digest)
                    || stored.4 != scope_count
                    || stored.5 != known_object_count
                    || stored.6 != 1
                    || stored.7 != runner_purge_request_id
                    || stored.8 != runner_authority.purge_generation
                    || stored.9 != runner_authority.tombstone_generation
                    || stored.10 != runner_authority.legacy_inventory_generation
                    || stored.11 != runner_authority.legacy_reconciliation_id
                    || stored.12 != runner_authority.legacy_authority_id
                    || !workflow_command_hmac_matches(
                        &stored.13,
                        &runner_authority.legacy_authority_sha256,
                    )
                    || !workflow_command_hmac_matches(
                        &stored.14,
                        &runner_authority.authority_digest,
                    )
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                for scope in &prepared {
                    let stored_scope = tx.query_row(
                        "SELECT manifest_digest_sha256, object_count, prefix_sweep
                           FROM jobs_workflow_cleanup_object_sweep_authorization_scopes
                          WHERE account_id = ?1 AND account_generation = ?2
                            AND sweep_attempt_id = ?3 AND scope_id = ?4",
                        params![
                            account_id,
                            requested_at_ms,
                            sweep_attempt_id,
                            scope.scope_id
                        ],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )?;
                    let stored_objects = {
                        let mut statement = tx.prepare(
                            "SELECT object_key_hmac_sha256
                               FROM jobs_workflow_cleanup_object_sweep_manifest_objects
                              WHERE account_id = ?1 AND account_generation = ?2
                                AND sweep_attempt_id = ?3 AND scope_id = ?4
                              ORDER BY object_key_hmac_sha256",
                        )?;
                        let rows = statement
                            .query_map(
                                params![
                                    account_id,
                                    requested_at_ms,
                                    sweep_attempt_id,
                                    scope.scope_id
                                ],
                                |row| row.get::<_, String>(0),
                            )?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                        rows
                    };
                    let mut expected_objects = scope.object_key_hmacs.clone();
                    expected_objects.sort();
                    if !workflow_command_hmac_matches(&stored_scope.0, &scope.manifest_digest)
                        || stored_scope.1 != i64::try_from(expected_objects.len())?
                        || stored_scope.2 != i64::from(scope.prefix_sweep)
                        || stored_objects != expected_objects
                    {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                true
            } else {
                tx.execute(
                    "INSERT INTO jobs_workflow_cleanup_object_sweep_authorizations (
                        account_id, account_generation, sweep_attempt_id,
                        runner_purge_request_id, runner_purge_generation,
                        runner_purge_tombstone_generation,
                        runner_legacy_inventory_generation,
                        runner_legacy_reconciliation_id, runner_legacy_authority_id,
                        runner_legacy_authority_sha256, runner_authority_digest_sha256,
                        completion_tombstone_id, completion_digest_sha256,
                        authorization_digest_sha256, scope_set_digest_sha256,
                        scope_count, known_object_count, sealed, authorized_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                        ?12, ?13, ?14, ?15, ?16, ?17, 0, ?18)",
                    params![
                        account_id,
                        requested_at_ms,
                        sweep_attempt_id,
                        runner_purge_request_id,
                        runner_authority.purge_generation,
                        runner_authority.tombstone_generation,
                        runner_authority.legacy_inventory_generation,
                        runner_authority.legacy_reconciliation_id,
                        runner_authority.legacy_authority_id,
                        runner_authority.legacy_authority_sha256,
                        runner_authority.authority_digest,
                        proof.tombstone_id,
                        proof.completion_digest,
                        authorization_digest,
                        scope_set_digest,
                        scope_count,
                        known_object_count,
                        now_ms,
                    ],
                )?;
                for scope in &prepared {
                    tx.execute(
                        "INSERT INTO jobs_workflow_cleanup_object_sweep_authorization_scopes (
                            account_id, account_generation, sweep_attempt_id, scope_id,
                            manifest_digest_sha256, object_count, prefix_sweep, created_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            account_id,
                            requested_at_ms,
                            sweep_attempt_id,
                            scope.scope_id,
                            scope.manifest_digest,
                            i64::try_from(scope.object_key_hmacs.len())?,
                            i64::from(scope.prefix_sweep),
                            now_ms,
                        ],
                    )?;
                    for object_key_hmac in &scope.object_key_hmacs {
                        tx.execute(
                            "INSERT INTO jobs_workflow_cleanup_object_sweep_manifest_objects (
                                account_id, account_generation, sweep_attempt_id, scope_id,
                                object_key_hmac_sha256, created_at_ms
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                account_id,
                                requested_at_ms,
                                sweep_attempt_id,
                                scope.scope_id,
                                object_key_hmac,
                                now_ms
                            ],
                        )?;
                    }
                }
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_object_sweep_authorizations
                        SET sealed = 1 WHERE account_id = ?1 AND account_generation = ?2
                         AND sweep_attempt_id = ?3 AND sealed = 0",
                    params![account_id, requested_at_ms, sweep_attempt_id],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_started_at_ms =
                              COALESCE(object_sweep_started_at_ms, ?1),
                            hard_delete_authorized_at_ms = NULL,
                            hard_delete_sweep_attempt_id = NULL,
                            hard_delete_authorization_digest_sha256 = NULL,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND account_generation = ?3",
                    params![now_ms, account_id, requested_at_ms],
                )?;
                false
            };
            status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                replayed,
            )?;
            status.replayed = replayed;
            tx.commit()?;
            Ok(JobsWorkflowCleanupObjectSweepBeginResult::Authorized(
                status,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            // Global runner fleet -> account/binding -> workflow global
            // authority is the shared lifecycle lock order.
            let fleet = tx.query_one(
                "SELECT legacy_inventory_state, legacy_inventory_generation,
                        legacy_inventory_reconciliation_id,
                        legacy_inventory_authority_id,
                        legacy_inventory_authority_sha256
                   FROM jobs_runner_volume_fleet_state
                  WHERE singleton_id = 1 FOR UPDATE",
                &[],
            )?;
            tx.query_one(
                "SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = $1 AND account_generation = $2 FOR UPDATE",
                &[&account_id, &requested_at_ms],
            )?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            let mut status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            if status.deletion_proof().as_ref() != Some(proof) {
                tx.commit()?;
                return Ok(JobsWorkflowCleanupObjectSweepBeginResult::PendingWorkflow(
                    status,
                ));
            }
            let runner = if fleet.get::<_, String>(0) == "ready" {
                tx.query_opt(
                    "SELECT request.purge_generation, tombstone.tombstone_generation,
                            request.legacy_inventory_generation,
                            request.legacy_inventory_reconciliation_id,
                            request.legacy_inventory_authority_id,
                            request.legacy_inventory_authority_sha256,
                            request.purge_subject, request.target_set_sha256,
                            request.required_target_count, request.completed_at_ms
                       FROM jobs_runner_purge_requests request
                       JOIN jobs_runner_purge_tombstones tombstone
                         ON tombstone.request_id = request.request_id
                        AND tombstone.purge_generation = request.purge_generation
                        AND tombstone.purge_subject = request.purge_subject
                        AND tombstone.target_set_sha256 = request.target_set_sha256
                        AND tombstone.required_target_count = request.required_target_count
                        AND tombstone.completed_at_ms = request.completed_at_ms
                      WHERE request.request_id = $1 AND request.account_id = $2
                        AND request.state = 'complete'
                        AND request.legacy_unresolved_count = 0
                        AND request.resolved_target_count = request.required_target_count
                        AND request.legacy_inventory_generation = $3
                        AND request.legacy_inventory_reconciliation_id = $4
                        AND request.legacy_inventory_authority_id = $5
                        AND request.legacy_inventory_authority_sha256 = $6
                      FOR SHARE OF request, tombstone",
                    &[
                        &runner_purge_request_id,
                        &account_id,
                        &fleet.get::<_, i64>(1),
                        &fleet.get::<_, Option<String>>(2),
                        &fleet.get::<_, Option<String>>(3),
                        &fleet.get::<_, Option<String>>(4),
                    ],
                )?
            } else {
                None
            };
            let Some(runner) = runner else {
                tx.commit()?;
                return Ok(JobsWorkflowCleanupObjectSweepBeginResult::PendingRunner(
                    status,
                ));
            };
            let runner_material = RunnerSweepAuthorityMaterial {
                purge_request_id: runner_purge_request_id.to_owned(),
                purge_subject: runner.get(6),
                purge_generation: runner.get(0),
                tombstone_generation: runner.get(1),
                target_set_digest: runner.get(7),
                required_target_count: runner.get(8),
                completed_at_ms: runner.get(9),
                legacy_inventory_generation: runner.get(2),
                legacy_reconciliation_id: runner.get(3),
                legacy_authority_id: runner.get(4),
                legacy_authority_sha256: runner.get(5),
            };
            let runner_authority = StoredRunnerSweepAuthority {
                purge_request_id: runner_material.purge_request_id.clone(),
                purge_generation: runner_material.purge_generation,
                tombstone_generation: runner_material.tombstone_generation,
                legacy_inventory_generation: runner_material.legacy_inventory_generation,
                legacy_reconciliation_id: runner_material.legacy_reconciliation_id.clone(),
                legacy_authority_id: runner_material.legacy_authority_id.clone(),
                legacy_authority_sha256: runner_material.legacy_authority_sha256.clone(),
                authority_digest: workflow_cleanup_runner_sweep_authority_digest(&runner_material)?,
            };
            let authorization_digest =
                workflow_cleanup_sweep_authorization_digest(&SweepAuthorizationDigestMaterial {
                    account_id,
                    requested_at_ms,
                    sweep_attempt_id,
                    scope_count: prepared.len(),
                    known_object_count,
                    scope_set_digest: &scope_set_digest,
                    runner: &runner_authority,
                    proof,
                })?;
            let existing = tx.query_opt(
                "SELECT completion_tombstone_id, completion_digest_sha256,
                        authorization_digest_sha256, scope_set_digest_sha256,
                        scope_count, known_object_count, sealed,
                        runner_purge_request_id, runner_purge_generation,
                        runner_purge_tombstone_generation,
                        runner_legacy_inventory_generation,
                        runner_legacy_reconciliation_id, runner_legacy_authority_id,
                        runner_legacy_authority_sha256, runner_authority_digest_sha256
                   FROM jobs_workflow_cleanup_object_sweep_authorizations
                  WHERE account_id = $1 AND account_generation = $2
                    AND sweep_attempt_id = $3 FOR UPDATE",
                &[&account_id, &requested_at_ms, &sweep_attempt_id],
            )?;
            let replayed = if let Some(stored) = existing {
                if stored.get::<_, String>(0) != proof.tombstone_id
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(1).as_str(),
                        &proof.completion_digest,
                    )
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(2).as_str(),
                        &authorization_digest,
                    )
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(3).as_str(),
                        &scope_set_digest,
                    )
                    || stored.get::<_, i64>(4) != scope_count
                    || stored.get::<_, i64>(5) != known_object_count
                    || !stored.get::<_, bool>(6)
                    || stored.get::<_, String>(7) != runner_purge_request_id
                    || stored.get::<_, i64>(8) != runner_authority.purge_generation
                    || stored.get::<_, i64>(9) != runner_authority.tombstone_generation
                    || stored.get::<_, i64>(10) != runner_authority.legacy_inventory_generation
                    || stored.get::<_, String>(11) != runner_authority.legacy_reconciliation_id
                    || stored.get::<_, String>(12) != runner_authority.legacy_authority_id
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(13).as_str(),
                        &runner_authority.legacy_authority_sha256,
                    )
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(14).as_str(),
                        &runner_authority.authority_digest,
                    )
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                for scope in &prepared {
                    let stored_scope = tx.query_one(
                        "SELECT manifest_digest_sha256, object_count, prefix_sweep
                           FROM jobs_workflow_cleanup_object_sweep_authorization_scopes
                          WHERE account_id = $1 AND account_generation = $2
                            AND sweep_attempt_id = $3 AND scope_id = $4 FOR SHARE",
                        &[
                            &account_id,
                            &requested_at_ms,
                            &sweep_attempt_id,
                            &scope.scope_id,
                        ],
                    )?;
                    let mut stored_objects = tx
                        .query(
                            "SELECT object_key_hmac_sha256
                           FROM jobs_workflow_cleanup_object_sweep_manifest_objects
                          WHERE account_id = $1 AND account_generation = $2
                            AND sweep_attempt_id = $3 AND scope_id = $4
                          ORDER BY object_key_hmac_sha256 FOR SHARE",
                            &[
                                &account_id,
                                &requested_at_ms,
                                &sweep_attempt_id,
                                &scope.scope_id,
                            ],
                        )?
                        .iter()
                        .map(|row| row.get::<_, String>(0))
                        .collect::<Vec<_>>();
                    let mut expected_objects = scope.object_key_hmacs.clone();
                    expected_objects.sort();
                    stored_objects.sort();
                    if !workflow_command_hmac_matches(
                        stored_scope.get::<_, String>(0).as_str(),
                        &scope.manifest_digest,
                    ) || stored_scope.get::<_, i64>(1) != i64::try_from(expected_objects.len())?
                        || stored_scope.get::<_, bool>(2) != scope.prefix_sweep
                        || stored_objects != expected_objects
                    {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                true
            } else {
                tx.execute(
                    "INSERT INTO jobs_workflow_cleanup_object_sweep_authorizations (
                        account_id, account_generation, sweep_attempt_id,
                        runner_purge_request_id, runner_purge_generation,
                        runner_purge_tombstone_generation,
                        runner_legacy_inventory_generation,
                        runner_legacy_reconciliation_id, runner_legacy_authority_id,
                        runner_legacy_authority_sha256, runner_authority_digest_sha256,
                        completion_tombstone_id, completion_digest_sha256,
                        authorization_digest_sha256, scope_set_digest_sha256,
                        scope_count, known_object_count, sealed, authorized_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                        $12, $13, $14, $15, $16, $17, FALSE, $18)",
                    &[
                        &account_id,
                        &requested_at_ms,
                        &sweep_attempt_id,
                        &runner_purge_request_id,
                        &runner_authority.purge_generation,
                        &runner_authority.tombstone_generation,
                        &runner_authority.legacy_inventory_generation,
                        &runner_authority.legacy_reconciliation_id,
                        &runner_authority.legacy_authority_id,
                        &runner_authority.legacy_authority_sha256,
                        &runner_authority.authority_digest,
                        &proof.tombstone_id,
                        &proof.completion_digest,
                        &authorization_digest,
                        &scope_set_digest,
                        &scope_count,
                        &known_object_count,
                        &now_ms,
                    ],
                )?;
                for scope in &prepared {
                    let object_count = i64::try_from(scope.object_key_hmacs.len())?;
                    tx.execute(
                        "INSERT INTO jobs_workflow_cleanup_object_sweep_authorization_scopes (
                            account_id, account_generation, sweep_attempt_id, scope_id,
                            manifest_digest_sha256, object_count, prefix_sweep, created_at_ms
                         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                        &[
                            &account_id,
                            &requested_at_ms,
                            &sweep_attempt_id,
                            &scope.scope_id,
                            &scope.manifest_digest,
                            &object_count,
                            &scope.prefix_sweep,
                            &now_ms,
                        ],
                    )?;
                    for object_key_hmac in &scope.object_key_hmacs {
                        tx.execute(
                            "INSERT INTO jobs_workflow_cleanup_object_sweep_manifest_objects (
                                account_id, account_generation, sweep_attempt_id, scope_id,
                                object_key_hmac_sha256, created_at_ms
                             ) VALUES ($1, $2, $3, $4, $5, $6)",
                            &[
                                &account_id,
                                &requested_at_ms,
                                &sweep_attempt_id,
                                &scope.scope_id,
                                object_key_hmac,
                                &now_ms,
                            ],
                        )?;
                    }
                }
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_object_sweep_authorizations
                        SET sealed = TRUE WHERE account_id = $1 AND account_generation = $2
                         AND sweep_attempt_id = $3 AND NOT sealed",
                    &[&account_id, &requested_at_ms, &sweep_attempt_id],
                )?;
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_started_at_ms =
                              COALESCE(object_sweep_started_at_ms, $1),
                            hard_delete_authorized_at_ms = NULL,
                            hard_delete_sweep_attempt_id = NULL,
                            hard_delete_authorization_digest_sha256 = NULL,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE account_id = $2 AND account_generation = $3",
                    &[&now_ms, &account_id, &requested_at_ms],
                )?;
                false
            };
            status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                replayed,
            )?;
            status.replayed = replayed;
            tx.commit()?;
            Ok(JobsWorkflowCleanupObjectSweepBeginResult::Authorized(
                status,
            ))
        }
    })
}

pub fn record_jobs_workflow_cleanup_object_sweep_progress(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    scope_id: &str,
    object_key: &str,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !workflow_command_opaque_identifier(sweep_attempt_id, 128)
        || !workflow_command_opaque_identifier(scope_id, 128)
        || object_key.is_empty()
        || object_key.len() > 2_048
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&caller_now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let object_key_hmac = workflow_command_hmac(
        "workflow-cleanup-object-sweep-key-v3",
        &json!({
            "accountGeneration": requested_at_ms,
            "accountId": account_id,
            "objectKey": object_key,
            "scopeId": scope_id,
        }),
        4_096,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let authorization_digest: String = tx.query_row(
                "SELECT sweep_auth.authorization_digest_sha256
                   FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
                   JOIN jobs_workflow_cleanup_object_sweep_manifest_objects object
                     ON object.account_id = sweep_auth.account_id
                    AND object.account_generation = sweep_auth.account_generation
                    AND object.sweep_attempt_id = sweep_auth.sweep_attempt_id
                  WHERE sweep_auth.account_id = ?1
                    AND sweep_auth.account_generation = ?2
                    AND sweep_auth.sweep_attempt_id = ?3
                    AND sweep_auth.sealed = 1 AND object.scope_id = ?4
                    AND object.object_key_hmac_sha256 = ?5",
                params![
                    account_id,
                    requested_at_ms,
                    sweep_attempt_id,
                    scope_id,
                    object_key_hmac
                ],
                |row| row.get(0),
            )?;
            let inserted = tx.execute(
                "INSERT INTO jobs_workflow_cleanup_object_sweep_progress (
                    account_id, account_generation, sweep_attempt_id,
                    authorization_digest_sha256, scope_id,
                    object_key_hmac_sha256, deleted_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(account_id, account_generation, scope_id,
                    object_key_hmac_sha256) DO NOTHING",
                params![
                    account_id,
                    requested_at_ms,
                    sweep_attempt_id,
                    authorization_digest,
                    scope_id,
                    object_key_hmac,
                    now_ms
                ],
            )?;
            if inserted == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_deleted_count = object_sweep_deleted_count + 1,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?2 AND account_generation = ?3",
                    params![now_ms, account_id, requested_at_ms],
                )?;
            }
            let status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            tx.commit()?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            tx.query_one(
                "SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = $1 AND account_generation = $2 FOR UPDATE",
                &[&account_id, &requested_at_ms],
            )?;
            let authorization_digest: String = tx
                .query_one(
                    "SELECT sweep_auth.authorization_digest_sha256
                       FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
                       JOIN jobs_workflow_cleanup_object_sweep_manifest_objects object
                         ON object.account_id = sweep_auth.account_id
                        AND object.account_generation = sweep_auth.account_generation
                        AND object.sweep_attempt_id = sweep_auth.sweep_attempt_id
                      WHERE sweep_auth.account_id = $1
                        AND sweep_auth.account_generation = $2
                        AND sweep_auth.sweep_attempt_id = $3
                        AND sweep_auth.sealed AND object.scope_id = $4
                        AND object.object_key_hmac_sha256 = $5
                      FOR SHARE OF sweep_auth, object",
                    &[
                        &account_id,
                        &requested_at_ms,
                        &sweep_attempt_id,
                        &scope_id,
                        &object_key_hmac,
                    ],
                )?
                .get(0);
            let inserted = tx.execute(
                "INSERT INTO jobs_workflow_cleanup_object_sweep_progress (
                    account_id, account_generation, sweep_attempt_id,
                    authorization_digest_sha256, scope_id,
                    object_key_hmac_sha256, deleted_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(account_id, account_generation, scope_id,
                    object_key_hmac_sha256) DO NOTHING",
                &[
                    &account_id,
                    &requested_at_ms,
                    &sweep_attempt_id,
                    &authorization_digest,
                    &scope_id,
                    &object_key_hmac,
                    &now_ms,
                ],
            )?;
            if inserted == 1 {
                tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_deleted_count = object_sweep_deleted_count + 1,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE account_id = $2 AND account_generation = $3",
                    &[&now_ms, &account_id, &requested_at_ms],
                )?;
            }
            let status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                true,
            )?;
            tx.commit()?;
            Ok(status)
        }
    })
}

// This public cross-slice contract deliberately keeps the exact sweep identity
// and the two independently audited result counts explicit for callers.
#[allow(clippy::too_many_arguments)]
pub fn record_jobs_workflow_cleanup_object_sweep_scope(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    scope_id: &str,
    deleted_count: i64,
    orphan_count: i64,
    caller_now_ms: i64,
) -> Result<JobsWorkflowCleanupDeletionStatus> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !workflow_command_opaque_identifier(sweep_attempt_id, 128)
        || !workflow_command_opaque_identifier(scope_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&deleted_count)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&orphan_count)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&caller_now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let result_digest = workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_SWEEP_SCOPE_DIGEST_DOMAIN,
        &json!({
            "accountGeneration": requested_at_ms,
            "accountId": account_id,
            "deletedCount": deleted_count,
            "orphanCount": orphan_count,
            "scopeId": scope_id,
            "sweepAttemptId": sweep_attempt_id,
        }),
        "workflow cleanup object sweep scope",
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now_ms = sqlite_workflow_cleanup_db_now_ms(&tx)?;
            let authorization_digest: String = tx.query_row(
                "SELECT sweep_auth.authorization_digest_sha256
                   FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
                   JOIN jobs_workflow_cleanup_object_sweep_authorization_scopes scope
                     ON scope.account_id = sweep_auth.account_id
                    AND scope.account_generation = sweep_auth.account_generation
                    AND scope.sweep_attempt_id = sweep_auth.sweep_attempt_id
                  WHERE sweep_auth.account_id = ?1
                    AND sweep_auth.account_generation = ?2
                    AND sweep_auth.sweep_attempt_id = ?3
                    AND sweep_auth.sealed = 1 AND scope.scope_id = ?4
                    AND scope.prefix_sweep = 1",
                params![account_id, requested_at_ms, sweep_attempt_id, scope_id],
                |row| row.get(0),
            )?;
            let existing = tx
                .query_row(
                    "SELECT deleted_count, orphan_count, result_digest_sha256
                       FROM jobs_workflow_cleanup_object_sweep_scopes
                      WHERE account_id = ?1 AND account_generation = ?2
                        AND sweep_attempt_id = ?3 AND scope_id = ?4",
                    params![account_id, requested_at_ms, sweep_attempt_id, scope_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let replayed = if let Some(stored) = existing {
                if stored.0 != deleted_count
                    || stored.1 != orphan_count
                    || !workflow_command_hmac_matches(&stored.2, &result_digest)
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                true
            } else {
                tx.execute(
                    "INSERT INTO jobs_workflow_cleanup_object_sweep_scopes (
                        account_id, account_generation, sweep_attempt_id,
                        authorization_digest_sha256, scope_id, deleted_count,
                        orphan_count, result_digest_sha256, recorded_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        account_id,
                        requested_at_ms,
                        sweep_attempt_id,
                        authorization_digest,
                        scope_id,
                        deleted_count,
                        orphan_count,
                        result_digest,
                        now_ms
                    ],
                )?;
                if tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_deleted_count = object_sweep_deleted_count + ?2,
                            object_sweep_orphan_count = object_sweep_orphan_count + ?3,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE account_id = ?4 AND account_generation = ?5",
                    params![
                        now_ms,
                        deleted_count,
                        orphan_count,
                        account_id,
                        requested_at_ms
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                }
                false
            };
            let mut status = workflow_cleanup_deletion_status_sqlite_tx(
                &tx,
                account_id,
                requested_at_ms,
                now_ms,
                replayed,
            )?;
            status.replayed = replayed;
            tx.commit()?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            let now_ms = postgres_workflow_cleanup_db_now_ms(&mut tx)?;
            tx.query_one(
                "SELECT 1 FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = $1 AND account_generation = $2 FOR UPDATE",
                &[&account_id, &requested_at_ms],
            )?;
            let authorization_digest: String = tx
                .query_one(
                    "SELECT sweep_auth.authorization_digest_sha256
                       FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
                       JOIN jobs_workflow_cleanup_object_sweep_authorization_scopes scope
                         ON scope.account_id = sweep_auth.account_id
                        AND scope.account_generation = sweep_auth.account_generation
                        AND scope.sweep_attempt_id = sweep_auth.sweep_attempt_id
                      WHERE sweep_auth.account_id = $1
                        AND sweep_auth.account_generation = $2
                        AND sweep_auth.sweep_attempt_id = $3
                        AND sweep_auth.sealed AND scope.scope_id = $4
                        AND scope.prefix_sweep
                      FOR SHARE OF sweep_auth, scope",
                    &[&account_id, &requested_at_ms, &sweep_attempt_id, &scope_id],
                )?
                .get(0);
            let existing = tx.query_opt(
                "SELECT deleted_count, orphan_count, result_digest_sha256
                   FROM jobs_workflow_cleanup_object_sweep_scopes
                  WHERE account_id = $1 AND account_generation = $2
                    AND sweep_attempt_id = $3 AND scope_id = $4 FOR SHARE",
                &[&account_id, &requested_at_ms, &sweep_attempt_id, &scope_id],
            )?;
            let replayed = if let Some(stored) = existing {
                if stored.get::<_, i64>(0) != deleted_count
                    || stored.get::<_, i64>(1) != orphan_count
                    || !workflow_command_hmac_matches(
                        stored.get::<_, String>(2).as_str(),
                        &result_digest,
                    )
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                true
            } else {
                tx.execute(
                    "INSERT INTO jobs_workflow_cleanup_object_sweep_scopes (
                        account_id, account_generation, sweep_attempt_id,
                        authorization_digest_sha256, scope_id, deleted_count,
                        orphan_count, result_digest_sha256, recorded_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    &[
                        &account_id,
                        &requested_at_ms,
                        &sweep_attempt_id,
                        &authorization_digest,
                        &scope_id,
                        &deleted_count,
                        &orphan_count,
                        &result_digest,
                        &now_ms,
                    ],
                )?;
                if tx.execute(
                    "UPDATE jobs_workflow_cleanup_account_bindings
                        SET object_sweep_deleted_count = object_sweep_deleted_count + $2,
                            object_sweep_orphan_count = object_sweep_orphan_count + $3,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE account_id = $4 AND account_generation = $5",
                    &[
                        &now_ms,
                        &deleted_count,
                        &orphan_count,
                        &account_id,
                        &requested_at_ms,
                    ],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::InvalidState.into());
                }
                false
            };
            let mut status = workflow_cleanup_deletion_status_postgres_tx(
                &mut tx,
                account_id,
                requested_at_ms,
                now_ms,
                replayed,
            )?;
            status.replayed = replayed;
            tx.commit()?;
            Ok(status)
        }
    })
}

pub fn get_jobs_workflow_cleanup_object_sweep_scope(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    scope_id: &str,
) -> Result<Option<JobsWorkflowCleanupObjectSweepScope>> {
    if !workflow_command_identifier(account_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !workflow_command_opaque_identifier(sweep_attempt_id, 128)
        || !workflow_command_opaque_identifier(scope_id, 128)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let connection = pool.get()?;
            Ok(connection
                .query_row(
                    "SELECT deleted_count, orphan_count, result_digest_sha256, recorded_at_ms
                       FROM jobs_workflow_cleanup_object_sweep_scopes
                      WHERE account_id = ?1 AND account_generation = ?2
                        AND sweep_attempt_id = ?3 AND scope_id = ?4",
                    params![account_id, requested_at_ms, sweep_attempt_id, scope_id],
                    |row| {
                        Ok(JobsWorkflowCleanupObjectSweepScope {
                            account_id: account_id.to_string(),
                            account_generation: requested_at_ms,
                            sweep_attempt_id: sweep_attempt_id.to_string(),
                            scope_id: scope_id.to_string(),
                            deleted_count: row.get(0)?,
                            orphan_count: row.get(1)?,
                            result_digest: row.get(2)?,
                            recorded_at_ms: row.get(3)?,
                        })
                    },
                )
                .optional()?)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            Ok(connection
                .query_opt(
                    "SELECT deleted_count, orphan_count, result_digest_sha256, recorded_at_ms
                       FROM jobs_workflow_cleanup_object_sweep_scopes
                      WHERE account_id = $1 AND account_generation = $2
                        AND sweep_attempt_id = $3 AND scope_id = $4",
                    &[&account_id, &requested_at_ms, &sweep_attempt_id, &scope_id],
                )?
                .map(|row| JobsWorkflowCleanupObjectSweepScope {
                    account_id: account_id.to_string(),
                    account_generation: requested_at_ms,
                    sweep_attempt_id: sweep_attempt_id.to_string(),
                    scope_id: scope_id.to_string(),
                    deleted_count: row.get(0),
                    orphan_count: row.get(1),
                    result_digest: row.get(2),
                    recorded_at_ms: row.get(3),
                }))
        }
    })
}

fn validate_jobs_workflow_cleanup_deletion_proof(
    account_id: &str,
    requested_at_ms: i64,
    proof: &JobsWorkflowCleanupDeletionProof,
) -> Result<()> {
    if !workflow_command_identifier(account_id, 128)
        || requested_at_ms != proof.account_generation
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&requested_at_ms)
        || !workflow_command_opaque_identifier(&proof.cleanup_generation_id, 128)
        || !workflow_cleanup_digest(&proof.target_set_digest)
        || !workflow_command_opaque_identifier(&proof.legacy_authority.inventory_generation_id, 128)
        || !workflow_cleanup_digest(&proof.legacy_authority.query_digest)
        || !workflow_command_opaque_identifier(&proof.tombstone_id, 128)
        || !workflow_cleanup_digest(&proof.completion_digest)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok(())
}

fn workflow_cleanup_hard_delete_authorization_digest(
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    sweep_authorization_digest: &str,
    proof: &JobsWorkflowCleanupDeletionProof,
) -> Result<String> {
    workflow_cleanup_sha256(
        WORKFLOW_CLEANUP_HARD_DELETE_AUTHORIZATION_DIGEST_DOMAIN,
        &json!({
            "accountGeneration": requested_at_ms,
            "accountId": account_id,
            "cleanupGenerationId": proof.cleanup_generation_id,
            "completionDigest": proof.completion_digest,
            "inventoryGenerationId": proof.legacy_authority.inventory_generation_id,
            "queryDigest": proof.legacy_authority.query_digest,
            "sweepAttemptId": sweep_attempt_id,
            "sweepAuthorizationDigest": sweep_authorization_digest,
            "targetSetDigest": proof.target_set_digest,
            "tombstoneId": proof.tombstone_id,
        }),
        "workflow cleanup hard delete authorization",
    )
}

pub(crate) fn require_account_deletion_workflow_cleanup_complete_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    proof: &JobsWorkflowCleanupDeletionProof,
) -> Result<()> {
    validate_jobs_workflow_cleanup_deletion_proof(account_id, requested_at_ms, proof)?;
    if !workflow_command_opaque_identifier(sweep_attempt_id, 128) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let complete: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM jobs_workflow_cleanup_account_bindings binding
           JOIN account_deletion_intents deletion
             ON deletion.account_id = binding.account_id
            AND deletion.requested_at_ms = binding.account_generation
           JOIN jobs_workflow_cleanup_completion_tombstones tombstone
             ON tombstone.tombstone_id = binding.completion_tombstone_id
            AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
            AND tombstone.account_generation = binding.account_generation
            AND tombstone.workflow_cleanup_generation = binding.workflow_cleanup_generation
            AND tombstone.cleanup_generation_id = binding.cleanup_generation_id
            AND tombstone.target_set_hmac_sha256 = binding.target_set_hmac_sha256
            AND tombstone.legacy_generation = binding.legacy_generation
            AND tombstone.legacy_inventory_generation_id =
                binding.legacy_inventory_generation_id
           JOIN jobs_workflow_legacy_inventory_head head ON head.singleton_id = 1
           JOIN jobs_workflow_legacy_inventory_generations legacy
             ON legacy.generation = head.generation
            AND legacy.inventory_generation_id = head.inventory_generation_id
            AND legacy.query_digest_sha256 = head.query_digest_sha256
            AND legacy.generation = binding.legacy_generation
            AND legacy.inventory_generation_id = binding.legacy_inventory_generation_id
            AND legacy.query_digest_sha256 = binding.legacy_query_digest_sha256
            AND legacy.completion_epoch = tombstone.legacy_completion_epoch
            AND legacy.completion_digest_sha256 = tombstone.legacy_completion_digest_sha256
          WHERE binding.account_id = ?1 AND binding.account_generation = ?2
            AND binding.cleanup_generation_id = ?3
            AND binding.target_set_hmac_sha256 = ?4
            AND binding.legacy_inventory_generation_id = ?5
            AND binding.legacy_query_digest_sha256 = ?6
            AND binding.completion_tombstone_id = ?7
            AND binding.completion_digest_sha256 = ?8
            AND binding.state = 'complete' AND legacy.state = 'complete'
            AND legacy.page_token_ciphertext IS NULL
            AND legacy.page_token_hmac_sha256 IS NULL
            AND legacy.revalidate_after_ms = tombstone.legacy_revalidate_after_ms
            AND legacy.revalidate_after_ms >
                CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
            AND NOT EXISTS (
              SELECT 1 FROM jobs_workflow_cleanup_targets target
               WHERE target.account_id = binding.account_id
                 AND target.generation = binding.workflow_cleanup_generation
                 AND target.target_set_hmac_sha256 = binding.target_set_hmac_sha256
                 AND target.target_state <> 'absence_proved'
            )
            AND NOT EXISTS (
              SELECT 1 FROM jobs_workflow_legacy_targets target
               WHERE target.generation = legacy.generation
                 AND (target.target_state <> 'absence_proved'
                   OR target.positive_reset_required = 1)
            )
            AND NOT EXISTS (
              SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
               WHERE page.generation = legacy.generation
                 AND page.raw_ciphertexts_scrubbed <> 1
            )
            AND NOT EXISTS (
              SELECT 1 FROM jobs_workflow_legacy_targets target
               WHERE target.generation = legacy.generation
                 AND target.raw_ids_scrubbed <> 1
            )",
        params![
            account_id,
            requested_at_ms,
            proof.cleanup_generation_id,
            proof.target_set_digest,
            proof.legacy_authority.inventory_generation_id,
            proof.legacy_authority.query_digest,
            proof.tombstone_id,
            proof.completion_digest,
        ],
        |row| row.get(0),
    )?;
    if complete != 1 {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    let sweep_authorization_digest: String = tx.query_row(
        "SELECT sweep.authorization_digest_sha256
           FROM jobs_workflow_cleanup_ready_object_sweeps sweep
           JOIN jobs_workflow_cleanup_account_bindings binding
             ON binding.account_id = sweep.account_id
            AND binding.account_generation = sweep.account_generation
            AND binding.completion_tombstone_id = sweep.completion_tombstone_id
            AND binding.completion_digest_sha256 = sweep.completion_digest_sha256
           JOIN jobs_workflow_cleanup_generations generation
             ON generation.account_id = binding.account_id
            AND generation.generation = binding.workflow_cleanup_generation
            AND generation.target_set_hmac_sha256 = binding.target_set_hmac_sha256
          WHERE sweep.account_id = ?1 AND sweep.account_generation = ?2
            AND sweep.sweep_attempt_id = ?3
            AND binding.cleanup_generation_id = ?4
            AND binding.target_set_hmac_sha256 = ?5
            AND binding.legacy_inventory_generation_id = ?6
            AND binding.legacy_query_digest_sha256 = ?7
            AND binding.completion_tombstone_id = ?8
            AND binding.completion_digest_sha256 = ?9
            AND generation.target_count = (
              SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
               WHERE target.account_id = generation.account_id
                 AND target.generation = generation.generation
                 AND target.target_set_hmac_sha256 = generation.target_set_hmac_sha256
            )
            AND generation.target_count = (
              SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_target_authorities authority
               WHERE authority.account_id = generation.account_id
                 AND authority.workflow_cleanup_generation = generation.generation
            )
            AND generation.target_count = (
              SELECT COUNT(*) FROM jobs_workflow_cleanup_v2_proved_targets proved
               WHERE proved.account_id = generation.account_id
                 AND proved.generation = generation.generation
                 AND proved.target_set_hmac_sha256 = generation.target_set_hmac_sha256
            )",
        params![
            account_id,
            requested_at_ms,
            sweep_attempt_id,
            proof.cleanup_generation_id,
            proof.target_set_digest,
            proof.legacy_authority.inventory_generation_id,
            proof.legacy_authority.query_digest,
            proof.tombstone_id,
            proof.completion_digest,
        ],
        |row| row.get(0),
    )?;
    let hard_delete_digest = workflow_cleanup_hard_delete_authorization_digest(
        account_id,
        requested_at_ms,
        sweep_attempt_id,
        &sweep_authorization_digest,
        proof,
    )?;
    let authority_now_ms = sqlite_workflow_cleanup_db_now_ms(tx)?;
    if tx.execute(
        "UPDATE jobs_workflow_cleanup_account_bindings
            SET hard_delete_authorized_at_ms = ?1,
                hard_delete_sweep_attempt_id = ?2,
                hard_delete_authorization_digest_sha256 = ?3,
                updated_at_ms = MAX(?1, updated_at_ms + 1)
          WHERE account_id = ?4 AND account_generation = ?5
            AND cleanup_generation_id = ?6
            AND completion_tombstone_id = ?7
            AND completion_digest_sha256 = ?8",
        params![
            authority_now_ms,
            sweep_attempt_id,
            hard_delete_digest,
            account_id,
            requested_at_ms,
            proof.cleanup_generation_id,
            proof.tombstone_id,
            proof.completion_digest,
        ],
    )? != 1
    {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    Ok(())
}

pub(crate) fn require_account_deletion_workflow_cleanup_complete_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    requested_at_ms: i64,
    sweep_attempt_id: &str,
    proof: &JobsWorkflowCleanupDeletionProof,
) -> Result<()> {
    validate_jobs_workflow_cleanup_deletion_proof(account_id, requested_at_ms, proof)?;
    if !workflow_command_opaque_identifier(sweep_attempt_id, 128) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    // The caller already holds runner-fleet then account. Continue in the
    // same order with account cleanup/tombstone before the global authority.
    // Periodic global revalidation never writes account bindings, so there is
    // no reverse global -> account lock edge.
    let binding = tx.query_opt(
        "SELECT binding.workflow_cleanup_generation, binding.legacy_generation,
                tombstone.legacy_completion_epoch,
                tombstone.legacy_completion_digest_sha256,
                tombstone.legacy_revalidate_after_ms
           FROM jobs_workflow_cleanup_account_bindings binding
           JOIN account_deletion_intents deletion
             ON deletion.account_id = binding.account_id
            AND deletion.requested_at_ms = binding.account_generation
           JOIN jobs_workflow_cleanup_completion_tombstones tombstone
             ON tombstone.tombstone_id = binding.completion_tombstone_id
            AND tombstone.completion_digest_sha256 = binding.completion_digest_sha256
          WHERE binding.account_id = $1 AND binding.account_generation = $2
            AND binding.cleanup_generation_id = $3
            AND binding.target_set_hmac_sha256 = $4
            AND binding.legacy_inventory_generation_id = $5
            AND binding.legacy_query_digest_sha256 = $6
            AND binding.completion_tombstone_id = $7
            AND binding.completion_digest_sha256 = $8
            AND binding.state = 'complete'
            AND tombstone.cleanup_generation_id = binding.cleanup_generation_id
            AND tombstone.target_set_hmac_sha256 = binding.target_set_hmac_sha256
            AND tombstone.account_generation = binding.account_generation
            AND tombstone.workflow_cleanup_generation = binding.workflow_cleanup_generation
            AND tombstone.legacy_generation = binding.legacy_generation
            AND tombstone.legacy_inventory_generation_id =
                binding.legacy_inventory_generation_id
          FOR UPDATE OF binding FOR SHARE OF deletion, tombstone",
        &[
            &account_id,
            &requested_at_ms,
            &proof.cleanup_generation_id,
            &proof.target_set_digest,
            &proof.legacy_authority.inventory_generation_id,
            &proof.legacy_authority.query_digest,
            &proof.tombstone_id,
            &proof.completion_digest,
        ],
    )?;
    let Some(binding) = binding else {
        anyhow::bail!("workflow cleanup completion is not current")
    };
    let workflow_cleanup_generation: i64 = binding.get(0);
    let legacy_generation: i64 = binding.get(1);
    let legacy_completion_epoch: i64 = binding.get(2);
    let legacy_completion_digest: String = binding.get(3);
    let legacy_revalidate_after_ms: i64 = binding.get(4);
    if tx
        .query_opt(
            "SELECT 1
               FROM jobs_workflow_legacy_inventory_head head
               JOIN jobs_workflow_legacy_inventory_generations legacy
                 ON legacy.generation = head.generation
                AND legacy.inventory_generation_id = head.inventory_generation_id
                AND legacy.query_digest_sha256 = head.query_digest_sha256
              WHERE head.singleton_id = 1 AND legacy.generation = $1
                AND legacy.inventory_generation_id = $2
                AND legacy.query_digest_sha256 = $3
                AND legacy.completion_epoch = $4
                AND legacy.completion_digest_sha256 = $5
                AND legacy.revalidate_after_ms = $6
                AND legacy.revalidate_after_ms >
                    FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint
                AND legacy.state = 'complete'
                AND legacy.page_token_ciphertext IS NULL
                AND legacy.page_token_hmac_sha256 IS NULL
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_inventory_pages page
                   WHERE page.generation = legacy.generation
                     AND NOT page.raw_ciphertexts_scrubbed
                )
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_workflow_legacy_targets target
                   WHERE target.generation = legacy.generation
                     AND NOT target.raw_ids_scrubbed
                )
              FOR SHARE OF head, legacy",
            &[
                &legacy_generation,
                &proof.legacy_authority.inventory_generation_id,
                &proof.legacy_authority.query_digest,
                &legacy_completion_epoch,
                &legacy_completion_digest,
                &legacy_revalidate_after_ms,
            ],
        )?
        .is_none()
    {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    let expected_target_count: i64 = tx
        .query_one(
            "SELECT target_count FROM jobs_workflow_cleanup_generations
              WHERE account_id = $1 AND generation = $2
                AND target_set_hmac_sha256 = $3 FOR SHARE",
            &[
                &account_id,
                &workflow_cleanup_generation,
                &proof.target_set_digest,
            ],
        )?
        .get(0);
    // Reconciliation workers lock the V2 authority before its companion
    // Phase609 target, so the deletion assertion preserves that order.
    let authorities = tx.query(
        "SELECT positive_reset_required
           FROM jobs_workflow_cleanup_v2_target_authorities
          WHERE account_id = $1 AND workflow_cleanup_generation = $2 FOR SHARE",
        &[&account_id, &workflow_cleanup_generation],
    )?;
    let targets = tx.query(
        "SELECT target_state FROM jobs_workflow_cleanup_targets
          WHERE account_id = $1 AND generation = $2
            AND target_set_hmac_sha256 = $3 FOR SHARE",
        &[
            &account_id,
            &workflow_cleanup_generation,
            &proof.target_set_digest,
        ],
    )?;
    if i64::try_from(authorities.len())? != expected_target_count
        || authorities
            .iter()
            .any(|authority| authority.get::<_, bool>(0))
        || i64::try_from(targets.len())? != expected_target_count
        || targets
            .iter()
            .any(|target| target.get::<_, String>(0) != "absence_proved")
    {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    let proved_target_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_v2_proved_targets
              WHERE account_id = $1 AND generation = $2
                AND target_set_hmac_sha256 = $3",
            &[
                &account_id,
                &workflow_cleanup_generation,
                &proof.target_set_digest,
            ],
        )?
        .get(0);
    if proved_target_count != expected_target_count {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    let legacy_targets = tx.query(
        "SELECT target_state, raw_ids_scrubbed, positive_reset_required
           FROM jobs_workflow_legacy_targets
          WHERE generation = $1 FOR SHARE",
        &[&legacy_generation],
    )?;
    if legacy_targets.iter().any(|target| {
        target.get::<_, String>(0) != "absence_proved"
            || !target.get::<_, bool>(1)
            || target.get::<_, bool>(2)
    }) {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    let sweep_authorization_digest: String = tx
        .query_one(
            "SELECT sweep_auth.authorization_digest_sha256
               FROM jobs_workflow_cleanup_object_sweep_authorizations sweep_auth
               JOIN jobs_workflow_cleanup_ready_object_sweeps sweep
                 ON sweep.account_id = sweep_auth.account_id
                AND sweep.account_generation = sweep_auth.account_generation
                AND sweep.sweep_attempt_id = sweep_auth.sweep_attempt_id
                AND sweep.authorization_digest_sha256 =
                    sweep_auth.authorization_digest_sha256
               JOIN jobs_workflow_cleanup_account_bindings binding
                 ON binding.account_id = sweep.account_id
                AND binding.account_generation = sweep.account_generation
                AND binding.completion_tombstone_id = sweep.completion_tombstone_id
                AND binding.completion_digest_sha256 = sweep.completion_digest_sha256
              WHERE sweep.account_id = $1 AND sweep.account_generation = $2
                AND sweep.sweep_attempt_id = $3
                AND binding.cleanup_generation_id = $4
                AND binding.target_set_hmac_sha256 = $5
                AND binding.legacy_inventory_generation_id = $6
                AND binding.legacy_query_digest_sha256 = $7
                AND binding.completion_tombstone_id = $8
                AND binding.completion_digest_sha256 = $9
              FOR SHARE OF sweep_auth",
            &[
                &account_id,
                &requested_at_ms,
                &sweep_attempt_id,
                &proof.cleanup_generation_id,
                &proof.target_set_digest,
                &proof.legacy_authority.inventory_generation_id,
                &proof.legacy_authority.query_digest,
                &proof.tombstone_id,
                &proof.completion_digest,
            ],
        )?
        .get(0);
    let hard_delete_digest = workflow_cleanup_hard_delete_authorization_digest(
        account_id,
        requested_at_ms,
        sweep_attempt_id,
        &sweep_authorization_digest,
        proof,
    )?;
    let authority_now_ms = postgres_workflow_cleanup_db_now_ms(tx)?;
    if tx.execute(
        "UPDATE jobs_workflow_cleanup_account_bindings
            SET hard_delete_authorized_at_ms = $1,
                hard_delete_sweep_attempt_id = $2,
                hard_delete_authorization_digest_sha256 = $3,
                updated_at_ms = GREATEST($1, updated_at_ms + 1)
          WHERE account_id = $4 AND account_generation = $5
            AND cleanup_generation_id = $6
            AND completion_tombstone_id = $7
            AND completion_digest_sha256 = $8",
        &[
            &authority_now_ms,
            &sweep_attempt_id,
            &hard_delete_digest,
            &account_id,
            &requested_at_ms,
            &proof.cleanup_generation_id,
            &proof.tombstone_id,
            &proof.completion_digest,
        ],
    )? != 1
    {
        anyhow::bail!("workflow cleanup completion is not current")
    }
    Ok(())
}

#[cfg(test)]
mod workflow_cleanup_authority_tests {
    use super::*;

    #[test]
    fn legacy_inventory_query_digest_matches_the_v3_cross_language_fixture() {
        let query = workflow_cleanup_visibility_query(1_783_900_800_000).unwrap();
        assert_eq!(query, "WorkflowType = \"applicationWorkflow\"");
        assert_eq!(
            workflow_cleanup_query_digest("bluey-jobs", 1_783_900_800_000, &query).unwrap(),
            "3c9d936edbb8c1f09a56b2278079f97d40731bf0d3d51a939bf2670598a32bf2"
        );
    }

    #[test]
    fn legacy_workflow_id_accepts_the_historical_envelope_only() {
        assert!(workflow_cleanup_legacy_workflow_id(
            "bluey-jobs:acct_123:key:part-456"
        ));
        let maximum = format!("bluey-jobs:{}:{}", "a".repeat(200), "b".repeat(200));
        assert_eq!(maximum.len(), 412);
        assert!(workflow_cleanup_legacy_workflow_id(&maximum));

        for invalid in [
            "bluey-jobs:aa:bbb".to_string(),
            "bluey-jobs:acct:bad\"key".to_string(),
            "bluey-jobs:acct:bad\\key".to_string(),
            format!("bluey-jobs:{}:bbb", "a".repeat(201)),
            format!("bluey-jobs:{}", "a".repeat(401)),
        ] {
            assert!(
                !workflow_cleanup_legacy_workflow_id(&invalid),
                "accepted invalid legacy workflow id: {invalid}"
            );
        }
    }

    #[test]
    fn v2_known_run_set_is_sorted_unique_and_bounded_to_32() {
        let run_ids = (0..WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS)
            .map(|index| format!("temporal-run-{index:03}-opaque"))
            .collect::<Vec<_>>();
        assert!(workflow_cleanup_sorted_unique_identifiers(
            &run_ids,
            WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS
        ));

        let mut overflow = run_ids.clone();
        overflow.push("temporal-run-032-opaque".to_string());
        assert!(!workflow_cleanup_sorted_unique_identifiers(
            &overflow,
            WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS
        ));

        let mut duplicate = run_ids;
        duplicate[31] = duplicate[30].clone();
        assert!(!workflow_cleanup_sorted_unique_identifiers(
            &duplicate,
            WORKFLOW_CLEANUP_MAX_V2_KNOWN_RUNS
        ));
    }

    #[test]
    fn managed_cloud_cleanup_memo_is_canonical_digest_and_binding_bound() {
        let binding_sha256 = "1".repeat(64);
        let memo = json!({
            "activationExpiresAtMs": 1_800_000_000_000_i64,
            "activationSha256": "2".repeat(64),
            "bindingSha256": binding_sha256,
            "channelSequence": 4,
            "cohortSha256": "3".repeat(64),
            "failureConverterSha256": "4".repeat(64),
            "headRevision": 5,
            "manifestSha256": "6".repeat(64),
            "readinessSha256": "7".repeat(64),
            "releaseId": "managed-cloud-release-test",
            "releaseSequence": 8,
            "resolvedAtMs": 1_700_000_000_000_i64,
            "scope": {
                "channel": "canary",
                "environment": "staging",
                "region": "us-east-1",
            },
            "taskQueueSha256": "8".repeat(64),
            "transitionSha256": "9".repeat(64),
            "trustGeneration": 2,
            "version": 1,
        });
        let bytes = serde_json::to_vec(&memo).unwrap();
        let base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes);
        let memo_sha256 = hex::encode(Sha256::digest(&bytes));
        assert!(validate_workflow_cleanup_managed_cloud_memo(
            Some(&"1".repeat(64)),
            Some(&base64url),
            Some(&memo_sha256),
        )
        .is_ok());
        assert!(validate_workflow_cleanup_managed_cloud_memo(None, None, None).is_ok());
        assert!(validate_workflow_cleanup_managed_cloud_memo(
            Some(&"2".repeat(64)),
            Some(&base64url),
            Some(&memo_sha256),
        )
        .is_err());
        assert!(validate_workflow_cleanup_managed_cloud_memo(
            Some(&"1".repeat(64)),
            Some(&base64url),
            None,
        )
        .is_err());
    }
}
