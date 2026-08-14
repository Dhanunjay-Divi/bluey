//! Disabled-by-default durable Jobs workflow-cleanup delivery.
//!
//! Account deletion and startup create database-owned cleanup authority. This
//! worker is the only server-to-Temporal-gateway network boundary and never
//! treats transport ambiguity as erasure evidence.

use std::{collections::BTreeMap, time::Duration};

use anyhow::Context;
use base64::Engine;
use futures_util::StreamExt;
use reqwest::{header::HeaderMap, redirect::Policy, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;

use crate::db::{
    jobs::{
        self, JobsLegacyInventoryPageLease, JobsLegacyTargetCleanupLease, JobsV2TargetCleanupLease,
        JobsWorkflowCleanupDeliveryFailure, JobsWorkflowCleanupWorkLease,
    },
    DbPool,
};

const CLEANUP_FLAG: &str = "BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED";
const ORIGIN_ENV: &str = "BLUEY_JOBS_WORKFLOW_ORIGIN";
const TOKEN_ENV: &str = "BLUEY_JOBS_WORKFLOW_TOKEN";
const NAMESPACE_ENV: &str = "BLUEY_JOBS_WORKFLOW_NAMESPACE";
const VISIBILITY_CUTOFF_ENV: &str = "BLUEY_JOBS_WORKFLOW_CLEANUP_VISIBILITY_CUTOFF_MS";
const POLL_SECONDS_ENV: &str = "BLUEY_JOBS_WORKFLOW_CLEANUP_POLL_SECONDS";
const LEASE_MS_ENV: &str = "BLUEY_JOBS_WORKFLOW_CLEANUP_LEASE_MS";
const CONFIRMATION_MS_ENV: &str = "BLUEY_JOBS_WORKFLOW_CLEANUP_CONFIRMATION_MS";

const DEFAULT_POLL_SECONDS: u64 = 5;
const DEFAULT_LEASE_MS: i64 = 30_000;
const DEFAULT_CONFIRMATION_MS: i64 = 30_000;
const MIN_LEASE_MS: i64 = 30_000;
const MAX_LEASE_MS: i64 = 5 * 60_000;
const MIN_CONFIRMATION_MS: i64 = 1_000;
const MAX_CONFIRMATION_MS: i64 = 10 * 60_000;
const MAX_VISIBILITY_CUTOFF_MS: i64 = 253_402_300_799_999;
const LEGACY_WORKFLOW_TYPE: &str = "applicationWorkflow";
const LEGACY_QUERY_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-query-v3\0";
const LEGACY_TARGETS_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-targets-v3\0";
const LEGACY_PAGE_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-inventory-page-v3\0";
const LEGACY_TARGET_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-legacy-target-v3\0";
const V2_TARGET_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-workflow-v2-target-v3\0";
const CLEANUP_EVIDENCE_DIGEST_DOMAIN: &[u8] = b"bluey-jobs-workflow-cleanup-evidence-v3\0";
const GATEWAY_REQUEST_BUDGET_MS: i64 = 12_000;
const HTTP_REQUEST_TIMEOUT_MS: i64 = 18_000;
const RECEIPT_PERSIST_MARGIN_MS: i64 = 10_000;
const _: () = assert!(HTTP_REQUEST_TIMEOUT_MS > GATEWAY_REQUEST_BUDGET_MS);
const _: () = assert!(MIN_LEASE_MS >= HTTP_REQUEST_TIMEOUT_MS + RECEIPT_PERSIST_MARGIN_MS);
const MAX_GATEWAY_BODY_BYTES: usize = 128 * 1024;
const CLAIM_BATCH_SIZE: usize = 25;
const MAX_PAGE_TOKEN_BYTES: usize = 4_096;
const MAX_INVENTORY_TARGETS: usize = 100;
const MAX_OBSERVED_RUN_IDS: usize = 32;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_LEGACY_INVENTORY_PAGES_PER_PASS: i64 = 4_096;
const MAX_PAGE_INDEX: i64 = MAX_LEGACY_INVENTORY_PAGES_PER_PASS - 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowCleanupOperation {
    LegacyInventoryPage,
    ReconcileLegacyTarget,
    ReconcileV2Target,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyInventoryPageRequest {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconcileLegacyTargetRequest {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconcileV2TargetRequest {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
    cleanup_request_id: String,
    cleanup_generation_id: String,
    target_set_digest: String,
    namespace: String,
    workflow_type: String,
    workflow_id: String,
    first_execution_run_id: Option<String>,
    start_request_id: String,
    start_payload_digest: String,
    known_run_ids: Vec<String>,
    target_digest: String,
    cleanup_fence: i64,
    observation_pass: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum WorkflowCleanupRequest {
    LegacyInventoryPage(LegacyInventoryPageRequest),
    ReconcileLegacyTarget(ReconcileLegacyTargetRequest),
    ReconcileV2Target(ReconcileV2TargetRequest),
}

impl WorkflowCleanupRequest {
    fn operation(&self) -> WorkflowCleanupOperation {
        match self {
            Self::LegacyInventoryPage(_) => WorkflowCleanupOperation::LegacyInventoryPage,
            Self::ReconcileLegacyTarget(_) => WorkflowCleanupOperation::ReconcileLegacyTarget,
            Self::ReconcileV2Target(_) => WorkflowCleanupOperation::ReconcileV2Target,
        }
    }
}

fn cleanup_request_for_lease(lease: &JobsWorkflowCleanupWorkLease) -> WorkflowCleanupRequest {
    match lease {
        JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => {
            WorkflowCleanupRequest::LegacyInventoryPage(legacy_inventory_request_for_lease(lease))
        }
        JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => {
            WorkflowCleanupRequest::ReconcileLegacyTarget(legacy_target_request_for_lease(lease))
        }
        JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => {
            WorkflowCleanupRequest::ReconcileV2Target(v2_target_request_for_lease(lease))
        }
    }
}

fn legacy_inventory_request_for_lease(
    lease: &JobsLegacyInventoryPageLease,
) -> LegacyInventoryPageRequest {
    LegacyInventoryPageRequest {
        schema_version: 3,
        operation: WorkflowCleanupOperation::LegacyInventoryPage,
        cleanup_request_id: lease.cleanup_request_id.clone(),
        inventory_generation_id: lease.inventory_generation_id.clone(),
        namespace: lease.namespace.clone(),
        workflow_type: lease.workflow_type.clone(),
        visibility_cutoff_ms: lease.visibility_cutoff_ms,
        query_digest: lease.query_digest.clone(),
        scan_pass: lease.scan_pass,
        page_index: lease.page_index,
        predecessor_page_digest: lease.predecessor_page_digest.clone(),
        page_token: lease.page_token.clone(),
        cleanup_fence: lease.cleanup_fence,
    }
}

fn legacy_target_request_for_lease(
    lease: &JobsLegacyTargetCleanupLease,
) -> ReconcileLegacyTargetRequest {
    ReconcileLegacyTargetRequest {
        schema_version: 3,
        operation: WorkflowCleanupOperation::ReconcileLegacyTarget,
        cleanup_request_id: lease.cleanup_request_id.clone(),
        inventory_generation_id: lease.inventory_generation_id.clone(),
        namespace: lease.namespace.clone(),
        workflow_type: lease.workflow_type.clone(),
        visibility_cutoff_ms: lease.visibility_cutoff_ms,
        query_digest: lease.query_digest.clone(),
        scan_pass: lease.scan_pass,
        workflow_id: lease.workflow_id.clone(),
        run_id: lease.run_id.clone(),
        first_execution_run_id: lease.first_execution_run_id.clone(),
        target_digest: lease.target_digest.clone(),
        cleanup_fence: lease.cleanup_fence,
        observation_pass: lease.observation_pass,
    }
}

fn v2_target_request_for_lease(lease: &JobsV2TargetCleanupLease) -> ReconcileV2TargetRequest {
    ReconcileV2TargetRequest {
        schema_version: 3,
        operation: WorkflowCleanupOperation::ReconcileV2Target,
        cleanup_request_id: lease.cleanup_request_id.clone(),
        cleanup_generation_id: lease.cleanup_generation_id.clone(),
        target_set_digest: lease.target_set_digest.clone(),
        namespace: lease.namespace.clone(),
        workflow_type: lease.workflow_type.clone(),
        workflow_id: lease.workflow_id.clone(),
        first_execution_run_id: lease.first_execution_run_id.clone(),
        start_request_id: lease.start_request_id.clone(),
        start_payload_digest: lease.start_payload_digest.clone(),
        known_run_ids: lease.known_run_ids.clone(),
        target_digest: lease.target_digest.clone(),
        cleanup_fence: lease.cleanup_fence,
        observation_pass: lease.observation_pass,
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TemporalWorkflowStatus {
    Running,
    Completed,
    Failed,
    Canceled,
    Terminated,
    ContinuedAsNew,
    TimedOut,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyInventoryTargetReceipt {
    workflow_id: String,
    run_id: String,
    first_execution_run_id: String,
    status: TemporalWorkflowStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyInventoryPageReceipt {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
    cleanup_request_id: String,
    inventory_generation_id: String,
    namespace: String,
    workflow_type: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    page_index: i64,
    #[serde(deserialize_with = "deserialize_present_nullable_string")]
    predecessor_page_digest: Option<String>,
    #[serde(deserialize_with = "deserialize_present_nullable_string")]
    page_token: Option<String>,
    cleanup_fence: i64,
    outcome: LegacyInventoryOutcome,
    page_digest: String,
    targets_digest: String,
    targets: Vec<LegacyInventoryTargetReceipt>,
    #[serde(deserialize_with = "deserialize_present_nullable_string")]
    next_page_token: Option<String>,
    exhausted: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LegacyInventoryOutcome {
    Page,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReconcileOutcome {
    Pending,
    AbsenceObserved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReconcileReason {
    WorkflowRunning,
    TerminationPending,
    HistoryDeletePending,
    VisibilityPending,
    TemporalUnavailable,
    AbsenceObserved,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconcileLegacyTargetReceipt {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
    cleanup_request_id: String,
    inventory_generation_id: String,
    namespace: String,
    workflow_type: String,
    visibility_cutoff_ms: i64,
    query_digest: String,
    scan_pass: i64,
    workflow_id: String,
    run_id: String,
    target_digest: String,
    cleanup_fence: i64,
    observation_pass: i64,
    outcome: ReconcileOutcome,
    reason: ReconcileReason,
    first_execution_run_id: String,
    run_ids: Vec<String>,
    evidence_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconcileV2TargetReceipt {
    schema_version: i64,
    operation: WorkflowCleanupOperation,
    cleanup_request_id: String,
    cleanup_generation_id: String,
    target_set_digest: String,
    namespace: String,
    workflow_type: String,
    workflow_id: String,
    start_request_id: String,
    start_payload_digest: String,
    known_run_ids: Vec<String>,
    target_digest: String,
    cleanup_fence: i64,
    observation_pass: i64,
    outcome: ReconcileOutcome,
    reason: ReconcileReason,
    #[serde(deserialize_with = "deserialize_present_nullable_string")]
    first_execution_run_id: Option<String>,
    run_ids: Vec<String>,
    evidence_digest: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowCleanupErrorOutcome {
    Rejected,
    IdentityConflict,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowCleanupErrorReason {
    InvalidRequest,
    NotFound,
    IdentityConflict,
    TemporalUnavailable,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowCleanupError {
    schema_version: i64,
    outcome: WorkflowCleanupErrorOutcome,
    reason: WorkflowCleanupErrorReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryableGatewayReason {
    TransportTimeout,
    ConnectionLost,
    GatewayUnavailable,
    GatewayServerError,
    MalformedResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermanentGatewayRejection {
    InvalidRequest,
    NotFound,
    IdentityConflict,
}

#[derive(Debug, PartialEq)]
enum ClassifiedGatewayResponse {
    Accepted(Value),
    Retryable(RetryableGatewayReason),
    Rejected(PermanentGatewayRejection),
}

fn deserialize_present_nullable_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Clone)]
struct CleanupDispatcherConfig {
    origin: Url,
    token: String,
    namespace: String,
    visibility_cutoff_ms: i64,
    query: String,
    query_digest: String,
    poll_interval: Duration,
    lease_ms: i64,
    confirmation_ms: i64,
}

impl CleanupDispatcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        let origin = std::env::var(ORIGIN_ENV).with_context(|| {
            format!("{ORIGIN_ENV} is required when workflow cleanup is enabled")
        })?;
        let origin = validated_gateway_origin(&origin)?;
        let token = std::env::var(TOKEN_ENV)
            .unwrap_or_default()
            .trim()
            .to_string();
        anyhow::ensure!(
            valid_gateway_token(&token),
            "{TOKEN_ENV} is invalid when workflow cleanup is enabled"
        );
        let namespace = std::env::var(NAMESPACE_ENV)
            .with_context(|| {
                format!("{NAMESPACE_ENV} is required when workflow cleanup is enabled")
            })?
            .trim()
            .to_string();
        anyhow::ensure!(
            valid_temporal_namespace(&namespace),
            "{NAMESPACE_ENV} is invalid when workflow cleanup is enabled"
        );
        let visibility_cutoff_ms =
            required_bounded_i64(VISIBILITY_CUTOFF_ENV, 1, MAX_VISIBILITY_CUTOFF_MS)?;
        let poll_seconds =
            optional_bounded_i64(POLL_SECONDS_ENV, DEFAULT_POLL_SECONDS as i64, 1, 300)?;
        let lease_ms =
            optional_bounded_i64(LEASE_MS_ENV, DEFAULT_LEASE_MS, MIN_LEASE_MS, MAX_LEASE_MS)?;
        let confirmation_ms = optional_bounded_i64(
            CONFIRMATION_MS_ENV,
            DEFAULT_CONFIRMATION_MS,
            MIN_CONFIRMATION_MS,
            MAX_CONFIRMATION_MS,
        )?;
        let query = legacy_inventory_query(visibility_cutoff_ms)?;
        let query_digest = legacy_inventory_query_digest(&namespace, visibility_cutoff_ms, &query)?;
        Ok(Self {
            origin,
            token,
            namespace,
            visibility_cutoff_ms,
            query,
            query_digest,
            poll_interval: Duration::from_secs(poll_seconds as u64),
            lease_ms,
            confirmation_ms,
        })
    }
}

/// Starts cleanup only behind its independent exact rollout gate. Disabled
/// startup neither validates gateway secrets nor creates inventory authority.
pub fn spawn_jobs_workflow_cleanup_dispatcher(
    pool: DbPool,
) -> anyhow::Result<Option<JoinHandle<()>>> {
    if !cleanup_enabled() {
        tracing::info!("Jobs workflow cleanup dispatcher disabled");
        return Ok(None);
    }
    let config = CleanupDispatcherConfig::from_env()?;
    let prepared = jobs::prepare_jobs_legacy_inventory_generation(
        &pool,
        &jobs::PrepareJobsLegacyInventoryGeneration {
            namespace: config.namespace.clone(),
            visibility_cutoff_ms: config.visibility_cutoff_ms,
            confirmation_age_ms: config.confirmation_ms,
            now_ms: jobs::now_ms(),
        },
    )?;
    anyhow::ensure!(
        prepared.namespace == config.namespace
            && prepared.workflow_type == LEGACY_WORKFLOW_TYPE
            && prepared.visibility_cutoff_ms == config.visibility_cutoff_ms
            && prepared.confirmation_age_ms == config.confirmation_ms
            && prepared.visibility_query == config.query
            && prepared.authority.query_digest == config.query_digest,
        "prepared Jobs workflow cleanup inventory authority does not match configuration"
    );
    let client = gateway_client()?;
    let owner = format!("jobs-workflow-cleanup-{}", uuid::Uuid::new_v4());
    Ok(Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if !cleanup_enabled() {
                continue;
            }
            if run_cleanup_dispatch_cycle(&pool, &client, &config, &owner)
                .await
                .is_err()
            {
                tracing::warn!(
                    reason_code = "workflow_cleanup_dispatch_cycle_database_failed",
                    "Jobs workflow cleanup dispatch cycle did not complete"
                );
            }
        }
    })))
}

async fn run_cleanup_dispatch_cycle(
    pool: &DbPool,
    client: &reqwest::Client,
    config: &CleanupDispatcherConfig,
    owner: &str,
) -> anyhow::Result<()> {
    for _ in 0..CLAIM_BATCH_SIZE {
        if !cleanup_enabled() {
            break;
        }
        let now_ms = jobs::now_ms();
        let Some(lease) =
            jobs::claim_jobs_workflow_cleanup_work(pool, owner, now_ms, config.lease_ms)?
        else {
            break;
        };
        if let Err(reason_code) = process_cleanup_work(pool, client, config, &lease).await {
            tracing::warn!(
                reason_code,
                "Jobs workflow cleanup delivery did not complete"
            );
        }
    }
    Ok(())
}

async fn process_cleanup_work(
    pool: &DbPool,
    client: &reqwest::Client,
    config: &CleanupDispatcherConfig,
    lease: &JobsWorkflowCleanupWorkLease,
) -> Result<(), &'static str> {
    if !cleanup_enabled() {
        return Ok(());
    }
    let now_ms = jobs::now_ms();
    if cleanup_lease_expires_at_ms(lease) <= now_ms {
        return Err("workflow_cleanup_lease_expired_before_request");
    }
    let request = cleanup_request_for_lease(lease);
    let request_is_valid = outbound_request_is_valid(config, &request);
    jobs::mark_jobs_workflow_cleanup_request_started(pool, lease, now_ms)
        .map_err(|_| "workflow_cleanup_request_start_evidence_failed")?;
    if !request_is_valid {
        jobs::record_jobs_workflow_cleanup_delivery_failure(
            pool,
            lease,
            JobsWorkflowCleanupDeliveryFailure::IdentityConflict,
            now_ms,
        )
        .map_err(|_| "workflow_cleanup_invalid_request_evidence_failed")?;
        return Err("workflow_cleanup_claimed_request_invalid");
    }
    // The durable request-start write can itself wait on the database. Re-read
    // time after it commits so no external request begins unless the same lease
    // still covers the full HTTP budget plus receipt-persistence margin.
    let delivery_started_at_ms = jobs::now_ms();
    if !cleanup_lease_has_delivery_budget(lease, delivery_started_at_ms) {
        jobs::record_jobs_workflow_cleanup_delivery_failure(
            pool,
            lease,
            JobsWorkflowCleanupDeliveryFailure::GatewayUnavailable,
            delivery_started_at_ms,
        )
        .map_err(|_| "workflow_cleanup_short_lease_release_failed")?;
        return Err("workflow_cleanup_lease_too_short_for_bounded_delivery");
    }

    if !cleanup_enabled() {
        jobs::record_jobs_workflow_cleanup_delivery_failure(
            pool,
            lease,
            JobsWorkflowCleanupDeliveryFailure::GatewayUnavailable,
            jobs::now_ms(),
        )
        .map_err(|_| "workflow_cleanup_disabled_release_failed")?;
        return Ok(());
    }
    let classified = deliver_cleanup_request(client, config, &request).await;
    let completed_at_ms = jobs::now_ms();
    match classified {
        ClassifiedGatewayResponse::Accepted(receipt) => {
            jobs::record_jobs_workflow_cleanup_receipt(pool, lease, &receipt, completed_at_ms)
                .map(|_| ())
                .map_err(|_| "workflow_cleanup_receipt_evidence_failed")
        }
        ClassifiedGatewayResponse::Retryable(reason) => {
            let failure = match reason {
                RetryableGatewayReason::GatewayUnavailable
                | RetryableGatewayReason::GatewayServerError => {
                    JobsWorkflowCleanupDeliveryFailure::GatewayUnavailable
                }
                RetryableGatewayReason::TransportTimeout
                | RetryableGatewayReason::ConnectionLost
                | RetryableGatewayReason::MalformedResponse => {
                    JobsWorkflowCleanupDeliveryFailure::TransportUnknown
                }
            };
            jobs::record_jobs_workflow_cleanup_delivery_failure(
                pool,
                lease,
                failure,
                completed_at_ms,
            )
            .map_err(|_| "workflow_cleanup_retry_evidence_failed")
        }
        ClassifiedGatewayResponse::Rejected(_reason) => {
            jobs::record_jobs_workflow_cleanup_delivery_failure(
                pool,
                lease,
                JobsWorkflowCleanupDeliveryFailure::IdentityConflict,
                completed_at_ms,
            )
            .map_err(|_| "workflow_cleanup_rejection_evidence_failed")
        }
    }
}

fn cleanup_lease_expires_at_ms(lease: &JobsWorkflowCleanupWorkLease) -> i64 {
    match lease {
        JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => lease.lease_expires_at_ms,
        JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => lease.lease_expires_at_ms,
        JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => lease.lease_expires_at_ms,
    }
}

fn cleanup_lease_has_delivery_budget(lease: &JobsWorkflowCleanupWorkLease, now_ms: i64) -> bool {
    cleanup_lease_expires_at_ms(lease).saturating_sub(now_ms)
        >= HTTP_REQUEST_TIMEOUT_MS + RECEIPT_PERSIST_MARGIN_MS
}

fn cleanup_enabled() -> bool {
    std::env::var(CLEANUP_FLAG).is_ok_and(|value| value == "true")
}

fn optional_bounded_i64(
    name: &str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> anyhow::Result<i64> {
    let value = match std::env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<i64>()
            .with_context(|| format!("{name} must be an integer"))?,
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => return Err(error).with_context(|| format!("read {name}")),
    };
    anyhow::ensure!(
        (minimum..=maximum).contains(&value),
        "{name} must be between {minimum} and {maximum}"
    );
    Ok(value)
}

fn required_bounded_i64(name: &str, minimum: i64, maximum: i64) -> anyhow::Result<i64> {
    let raw = std::env::var(name).with_context(|| format!("{name} is required"))?;
    let value = raw
        .trim()
        .parse::<i64>()
        .with_context(|| format!("{name} must be an integer"))?;
    anyhow::ensure!(
        (minimum..=maximum).contains(&value),
        "{name} must be between {minimum} and {maximum}"
    );
    Ok(value)
}

fn valid_gateway_token(token: &str) -> bool {
    if !(32..=8 * 1024).contains(&token.len()) || !token.is_ascii() {
        return false;
    }
    let authority = token.trim_end_matches('=');
    !authority.is_empty()
        && authority.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
        })
}

fn valid_temporal_namespace(namespace: &str) -> bool {
    (1..=255).contains(&namespace.len())
        && namespace.is_ascii()
        && namespace
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validated_gateway_origin(raw: &str) -> anyhow::Result<Url> {
    let mut origin = Url::parse(raw.trim()).with_context(|| format!("parse {ORIGIN_ENV}"))?;
    anyhow::ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "{ORIGIN_ENV} must not contain credentials, query, or fragment"
    );
    let local_debug_origin = cfg!(debug_assertions)
        && origin.scheme() == "http"
        && matches!(origin.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    anyhow::ensure!(
        origin.scheme() == "https" || local_debug_origin,
        "{ORIGIN_ENV} must use HTTPS"
    );
    anyhow::ensure!(
        matches!(origin.path(), "" | "/"),
        "{ORIGIN_ENV} must not contain a path"
    );
    origin.set_path("/");
    Ok(origin)
}

fn legacy_inventory_query(_visibility_cutoff_ms: i64) -> anyhow::Result<String> {
    // The cutoff is a signed cutover-policy input and remains in the authority
    // digest. It must never filter inventory: late and continued-as-new v1 runs
    // are global erasure blockers too.
    Ok(format!("WorkflowType = \"{LEGACY_WORKFLOW_TYPE}\""))
}

fn legacy_inventory_query_digest(
    namespace: &str,
    visibility_cutoff_ms: i64,
    query: &str,
) -> anyhow::Result<String> {
    let canonical = BTreeMap::from([
        ("namespace", Value::String(namespace.to_string())),
        ("query", Value::String(query.to_string())),
        (
            "visibilityCutoffMs",
            Value::Number(visibility_cutoff_ms.into()),
        ),
        (
            "workflowType",
            Value::String(LEGACY_WORKFLOW_TYPE.to_string()),
        ),
    ]);
    cleanup_domain_digest(
        LEGACY_QUERY_DIGEST_DOMAIN,
        &serde_json::to_value(canonical).context("serialize legacy inventory query")?,
    )
}

fn cleanup_domain_digest(domain: &[u8], value: &Value) -> anyhow::Result<String> {
    let canonical = canonicalize_cleanup_evidence(value)?;
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(canonical.as_bytes());
    Ok(hex::encode(digest.finalize()))
}

fn canonicalize_cleanup_evidence(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => {
            serde_json::to_string(value).context("serialize workflow cleanup evidence")
        }
        Value::Number(number) => {
            let valid_signed = number
                .as_i64()
                .is_some_and(|value| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value));
            let valid_unsigned = number
                .as_u64()
                .is_some_and(|value| value <= MAX_SAFE_INTEGER as u64);
            anyhow::ensure!(
                valid_signed || valid_unsigned,
                "cleanup number is not a safe integer"
            );
            Ok(number.to_string())
        }
        Value::Array(values) => {
            let values = values
                .iter()
                .map(canonicalize_cleanup_evidence)
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(format!("[{}]", values.join(",")))
        }
        Value::Object(values) => {
            let fields = values
                .iter()
                .map(|(key, value)| {
                    Ok((
                        key.as_str(),
                        format!(
                            "{}:{}",
                            serde_json::to_string(key)?,
                            canonicalize_cleanup_evidence(value)?
                        ),
                    ))
                })
                .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
            Ok(format!(
                "{{{}}}",
                fields.into_values().collect::<Vec<_>>().join(",")
            ))
        }
    }
}

fn gateway_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_millis(HTTP_REQUEST_TIMEOUT_MS as u64))
        .redirect(Policy::none())
        .user_agent("bluey-jobs-workflow-cleanup/3")
        .build()
        .context("build Jobs workflow cleanup gateway client")
}

async fn deliver_cleanup_request(
    client: &reqwest::Client,
    config: &CleanupDispatcherConfig,
    request: &WorkflowCleanupRequest,
) -> ClassifiedGatewayResponse {
    if !outbound_request_is_valid(config, request) {
        return ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::InvalidRequest);
    }
    let endpoint = match config.origin.join("workflow-cleanup") {
        Ok(endpoint) => endpoint,
        Err(_) => {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::GatewayUnavailable)
        }
    };
    let response = match client
        .post(endpoint)
        .bearer_auth(&config.token)
        .json(request)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::TransportTimeout)
        }
        Err(error) if error.is_connect() => {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::ConnectionLost)
        }
        Err(_) => {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::GatewayUnavailable)
        }
    };
    let status = response.status();
    let response_headers_are_exact = gateway_response_headers_are_exact(response.headers());
    let body = match bounded_response_body(response).await {
        Ok(body) => body,
        Err(()) => {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
        }
    };
    classify_gateway_response(request, status, response_headers_are_exact, &body)
}

fn outbound_request_is_valid(
    config: &CleanupDispatcherConfig,
    request: &WorkflowCleanupRequest,
) -> bool {
    match request {
        WorkflowCleanupRequest::LegacyInventoryPage(request) => {
            request.schema_version == 3
                && request.operation == WorkflowCleanupOperation::LegacyInventoryPage
                && valid_opaque_id(&request.cleanup_request_id, 128)
                && valid_opaque_id(&request.inventory_generation_id, 128)
                && request.namespace == config.namespace
                && request.workflow_type == LEGACY_WORKFLOW_TYPE
                && request.visibility_cutoff_ms == config.visibility_cutoff_ms
                && request.query_digest == config.query_digest
                && matches!(request.scan_pass, 1 | 2)
                && (0..=MAX_PAGE_INDEX).contains(&request.page_index)
                && valid_optional_digest(request.predecessor_page_digest.as_deref())
                && canonical_page_token(request.page_token.as_deref())
                && ((request.page_index == 0
                    && request.predecessor_page_digest.is_none()
                    && request.page_token.is_none())
                    || (request.page_index > 0
                        && request.predecessor_page_digest.is_some()
                        && request.page_token.is_some()))
                && positive_safe_integer(request.cleanup_fence)
        }
        WorkflowCleanupRequest::ReconcileLegacyTarget(request) => {
            request.schema_version == 3
                && request.operation == WorkflowCleanupOperation::ReconcileLegacyTarget
                && valid_opaque_id(&request.cleanup_request_id, 128)
                && valid_opaque_id(&request.inventory_generation_id, 128)
                && request.namespace == config.namespace
                && request.workflow_type == LEGACY_WORKFLOW_TYPE
                && request.visibility_cutoff_ms == config.visibility_cutoff_ms
                && request.query_digest == config.query_digest
                && matches!(request.scan_pass, 1 | 2)
                && valid_legacy_workflow_id(&request.workflow_id)
                && valid_opaque_id(&request.run_id, 128)
                && valid_opaque_id(&request.first_execution_run_id, 128)
                && valid_digest(&request.target_digest)
                && request_target_digest_matches(
                    request,
                    LEGACY_TARGET_DIGEST_DOMAIN,
                    &request.target_digest,
                    &[],
                )
                && positive_safe_integer(request.cleanup_fence)
                && matches!(request.observation_pass, 1 | 2)
        }
        WorkflowCleanupRequest::ReconcileV2Target(request) => {
            request.schema_version == 3
                && request.operation == WorkflowCleanupOperation::ReconcileV2Target
                && valid_opaque_id(&request.cleanup_request_id, 128)
                && valid_opaque_id(&request.cleanup_generation_id, 128)
                && valid_digest(&request.target_set_digest)
                && request.namespace == config.namespace
                && request.workflow_type == "applicationWorkflowV2"
                && valid_opaque_id(&request.workflow_id, 192)
                && request
                    .first_execution_run_id
                    .as_deref()
                    .is_none_or(|value| valid_opaque_id(value, 128))
                && valid_opaque_id(&request.start_request_id, 128)
                && valid_digest(&request.start_payload_digest)
                && closed_run_id_set(&request.known_run_ids)
                && match request.first_execution_run_id.as_deref() {
                    Some(value) => request.known_run_ids.iter().any(|run_id| run_id == value),
                    None => request.known_run_ids.is_empty(),
                }
                && valid_digest(&request.target_digest)
                && request_target_digest_matches(
                    request,
                    V2_TARGET_DIGEST_DOMAIN,
                    &request.target_digest,
                    &["knownRunIds"],
                )
                && positive_safe_integer(request.cleanup_fence)
                && matches!(request.observation_pass, 1 | 2)
        }
    }
}

fn request_target_digest_matches<T: Serialize>(
    request: &T,
    domain: &[u8],
    expected: &str,
    additional_mutable_fields: &[&str],
) -> bool {
    let Ok(mut value) = serde_json::to_value(request) else {
        return false;
    };
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    for key in [
        "schemaVersion",
        "operation",
        "cleanupRequestId",
        "targetDigest",
        "cleanupFence",
        "observationPass",
    ] {
        if object.remove(key).is_none() {
            return false;
        }
    }
    for key in additional_mutable_fields {
        if object.remove(*key).is_none() {
            return false;
        }
    }
    cleanup_domain_digest(domain, &value).is_ok_and(|digest| digest == expected)
}

fn positive_safe_integer(value: i64) -> bool {
    (1..=MAX_SAFE_INTEGER).contains(&value)
}

fn valid_optional_digest(value: Option<&str>) -> bool {
    value.is_none_or(valid_digest)
}

fn classify_gateway_response(
    request: &WorkflowCleanupRequest,
    status: StatusCode,
    response_headers_are_exact: bool,
    body: &[u8],
) -> ClassifiedGatewayResponse {
    if !response_headers_are_exact {
        return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse);
    }
    if status == StatusCode::ACCEPTED {
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse);
        };
        let accepted = match request {
            WorkflowCleanupRequest::LegacyInventoryPage(request) => {
                serde_json::from_value::<LegacyInventoryPageReceipt>(value.clone())
                    .ok()
                    .is_some_and(|receipt| inventory_receipt_matches(request, &receipt))
            }
            WorkflowCleanupRequest::ReconcileLegacyTarget(request) => {
                serde_json::from_value::<ReconcileLegacyTargetReceipt>(value.clone())
                    .ok()
                    .is_some_and(|receipt| legacy_reconcile_receipt_matches(request, &receipt))
            }
            WorkflowCleanupRequest::ReconcileV2Target(request) => {
                serde_json::from_value::<ReconcileV2TargetReceipt>(value.clone())
                    .ok()
                    .is_some_and(|receipt| v2_reconcile_receipt_matches(request, &receipt))
            }
        } && response_digests_match(request.operation(), &value);
        return if accepted {
            ClassifiedGatewayResponse::Accepted(value)
        } else {
            ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
        };
    }
    if status.is_success() {
        return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse);
    }
    if status.is_server_error() {
        return ClassifiedGatewayResponse::Retryable(
            if status == StatusCode::SERVICE_UNAVAILABLE {
                RetryableGatewayReason::GatewayUnavailable
            } else {
                RetryableGatewayReason::GatewayServerError
            },
        );
    }
    if status == StatusCode::TOO_MANY_REQUESTS
        || matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
    {
        return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::GatewayUnavailable);
    }
    let Some(error) = serde_json::from_slice::<WorkflowCleanupError>(body)
        .ok()
        .filter(|error| error.schema_version == 3)
    else {
        return ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse);
    };
    match (status, error.outcome, error.reason) {
        (
            StatusCode::BAD_REQUEST,
            WorkflowCleanupErrorOutcome::Rejected,
            WorkflowCleanupErrorReason::InvalidRequest,
        ) => ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::InvalidRequest),
        (
            StatusCode::NOT_FOUND,
            WorkflowCleanupErrorOutcome::Rejected,
            WorkflowCleanupErrorReason::NotFound,
        ) => ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::NotFound),
        (
            StatusCode::CONFLICT,
            WorkflowCleanupErrorOutcome::IdentityConflict,
            WorkflowCleanupErrorReason::IdentityConflict,
        ) => ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::IdentityConflict),
        _ => ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse),
    }
}

fn response_digests_match(operation: WorkflowCleanupOperation, value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    match operation {
        WorkflowCleanupOperation::LegacyInventoryPage => {
            let Some(targets) = object.get("targets") else {
                return false;
            };
            let Some(expected_targets_digest) = object.get("targetsDigest").and_then(Value::as_str)
            else {
                return false;
            };
            let Some(expected_page_digest) = object.get("pageDigest").and_then(Value::as_str)
            else {
                return false;
            };
            let Ok(targets_digest) = cleanup_domain_digest(LEGACY_TARGETS_DIGEST_DOMAIN, targets)
            else {
                return false;
            };
            let mut page = value.clone();
            let Some(page) = page.as_object_mut() else {
                return false;
            };
            if page.remove("pageDigest").is_none() {
                return false;
            }
            targets_digest == expected_targets_digest
                && cleanup_domain_digest(LEGACY_PAGE_DIGEST_DOMAIN, &Value::Object(page.clone()))
                    .is_ok_and(|digest| digest == expected_page_digest)
        }
        WorkflowCleanupOperation::ReconcileLegacyTarget
        | WorkflowCleanupOperation::ReconcileV2Target => {
            let Some(expected) = object.get("evidenceDigest").and_then(Value::as_str) else {
                return false;
            };
            let mut evidence = value.clone();
            let Some(evidence) = evidence.as_object_mut() else {
                return false;
            };
            if evidence.remove("evidenceDigest").is_none() {
                return false;
            }
            cleanup_domain_digest(
                CLEANUP_EVIDENCE_DIGEST_DOMAIN,
                &Value::Object(evidence.clone()),
            )
            .is_ok_and(|digest| digest == expected)
        }
    }
}

fn inventory_receipt_matches(
    request: &LegacyInventoryPageRequest,
    receipt: &LegacyInventoryPageReceipt,
) -> bool {
    receipt.schema_version == 3
        && receipt.operation == WorkflowCleanupOperation::LegacyInventoryPage
        && request.operation == receipt.operation
        && receipt.cleanup_request_id == request.cleanup_request_id
        && receipt.inventory_generation_id == request.inventory_generation_id
        && receipt.namespace == request.namespace
        && receipt.workflow_type == LEGACY_WORKFLOW_TYPE
        && receipt.workflow_type == request.workflow_type
        && receipt.visibility_cutoff_ms == request.visibility_cutoff_ms
        && receipt.query_digest == request.query_digest
        && receipt.scan_pass == request.scan_pass
        && receipt.page_index == request.page_index
        && (0..=MAX_PAGE_INDEX).contains(&receipt.page_index)
        && receipt.predecessor_page_digest == request.predecessor_page_digest
        && receipt.page_token == request.page_token
        && receipt.cleanup_fence == request.cleanup_fence
        && receipt.outcome == LegacyInventoryOutcome::Page
        && valid_digest(&receipt.page_digest)
        && valid_digest(&receipt.targets_digest)
        && receipt.targets.len() <= MAX_INVENTORY_TARGETS
        && canonical_page_token(receipt.next_page_token.as_deref())
        && receipt.exhausted == receipt.next_page_token.is_none()
        && (receipt.page_index < MAX_PAGE_INDEX || receipt.next_page_token.is_none())
        && inventory_targets_are_closed(&receipt.targets)
}

fn inventory_targets_are_closed(targets: &[LegacyInventoryTargetReceipt]) -> bool {
    let mut previous: Option<(&str, &str)> = None;
    for target in targets {
        if !valid_legacy_workflow_id(&target.workflow_id)
            || !valid_opaque_id(&target.run_id, 128)
            || !valid_opaque_id(&target.first_execution_run_id, 128)
        {
            return false;
        }
        let current = (target.workflow_id.as_str(), target.run_id.as_str());
        if previous.is_some_and(|value| value >= current) {
            return false;
        }
        previous = Some(current);
        match target.status {
            TemporalWorkflowStatus::Running
            | TemporalWorkflowStatus::Completed
            | TemporalWorkflowStatus::Failed
            | TemporalWorkflowStatus::Canceled
            | TemporalWorkflowStatus::Terminated
            | TemporalWorkflowStatus::ContinuedAsNew
            | TemporalWorkflowStatus::TimedOut => {}
        }
    }
    true
}

fn legacy_reconcile_receipt_matches(
    request: &ReconcileLegacyTargetRequest,
    receipt: &ReconcileLegacyTargetReceipt,
) -> bool {
    receipt.schema_version == 3
        && receipt.operation == WorkflowCleanupOperation::ReconcileLegacyTarget
        && request.operation == receipt.operation
        && receipt.cleanup_request_id == request.cleanup_request_id
        && receipt.inventory_generation_id == request.inventory_generation_id
        && receipt.namespace == request.namespace
        && receipt.workflow_type == LEGACY_WORKFLOW_TYPE
        && receipt.workflow_type == request.workflow_type
        && receipt.visibility_cutoff_ms == request.visibility_cutoff_ms
        && receipt.query_digest == request.query_digest
        && receipt.scan_pass == request.scan_pass
        && receipt.workflow_id == request.workflow_id
        && valid_legacy_workflow_id(&receipt.workflow_id)
        && receipt.run_id == request.run_id
        && receipt.target_digest == request.target_digest
        && receipt.cleanup_fence == request.cleanup_fence
        && receipt.observation_pass == request.observation_pass
        && receipt.run_ids == [request.run_id.clone()]
        && reconcile_receipt_tail_matches(
            Some(request.first_execution_run_id.as_str()),
            Some(request.run_id.as_str()),
            false,
            &[],
            WorkflowCleanupOperation::ReconcileLegacyTarget,
            Some(receipt.first_execution_run_id.as_str()),
            &receipt.run_ids,
            receipt.outcome,
            receipt.reason,
            &receipt.evidence_digest,
        )
}

fn v2_reconcile_receipt_matches(
    request: &ReconcileV2TargetRequest,
    receipt: &ReconcileV2TargetReceipt,
) -> bool {
    receipt.schema_version == 3
        && receipt.operation == WorkflowCleanupOperation::ReconcileV2Target
        && request.operation == receipt.operation
        && receipt.cleanup_request_id == request.cleanup_request_id
        && receipt.cleanup_generation_id == request.cleanup_generation_id
        && receipt.target_set_digest == request.target_set_digest
        && receipt.namespace == request.namespace
        && receipt.workflow_type == "applicationWorkflowV2"
        && receipt.workflow_type == request.workflow_type
        && receipt.workflow_id == request.workflow_id
        && receipt.start_request_id == request.start_request_id
        && receipt.start_payload_digest == request.start_payload_digest
        && receipt.known_run_ids == request.known_run_ids
        && receipt.target_digest == request.target_digest
        && receipt.cleanup_fence == request.cleanup_fence
        && receipt.observation_pass == request.observation_pass
        && (!(receipt.run_ids.len() > request.known_run_ids.len()
            || (request.first_execution_run_id.is_none()
                && receipt.first_execution_run_id.is_some()))
            || (receipt.outcome == ReconcileOutcome::Pending
                && receipt.reason == ReconcileReason::VisibilityPending))
        && reconcile_receipt_tail_matches(
            request.first_execution_run_id.as_deref(),
            None,
            true,
            &request.known_run_ids,
            WorkflowCleanupOperation::ReconcileV2Target,
            receipt.first_execution_run_id.as_deref(),
            &receipt.run_ids,
            receipt.outcome,
            receipt.reason,
            &receipt.evidence_digest,
        )
}

#[allow(clippy::too_many_arguments)]
fn reconcile_receipt_tail_matches(
    expected_first_run_id: Option<&str>,
    required_run_id: Option<&str>,
    require_first_run_membership: bool,
    required_known_run_ids: &[String],
    operation: WorkflowCleanupOperation,
    observed_first_run_id: Option<&str>,
    run_ids: &[String],
    outcome: ReconcileOutcome,
    reason: ReconcileReason,
    evidence_digest: &str,
) -> bool {
    if expected_first_run_id.is_some() && expected_first_run_id != observed_first_run_id {
        return false;
    }
    let first_run_binding_is_split = require_first_run_membership
        && match observed_first_run_id {
            Some(_) => run_ids.is_empty(),
            None => !run_ids.is_empty(),
        };
    if observed_first_run_id.is_some_and(|value| !valid_opaque_id(value, 128))
        || run_ids.len() > MAX_OBSERVED_RUN_IDS
        || !valid_digest(evidence_digest)
        || !reconcile_outcome_reason_is_valid(operation, outcome, reason)
        || first_run_binding_is_split
    {
        return false;
    }
    if !closed_run_id_set(run_ids) {
        return false;
    }
    required_run_id.is_none_or(|value| run_ids.iter().any(|run_id| run_id == value))
        && required_known_run_ids
            .iter()
            .all(|known| run_ids.iter().any(|observed| observed == known))
        && (!require_first_run_membership
            || observed_first_run_id
                .is_none_or(|value| run_ids.iter().any(|run_id| run_id == value)))
}

fn reconcile_outcome_reason_is_valid(
    operation: WorkflowCleanupOperation,
    outcome: ReconcileOutcome,
    reason: ReconcileReason,
) -> bool {
    matches!(
        (operation, outcome, reason),
        (
            WorkflowCleanupOperation::ReconcileLegacyTarget
                | WorkflowCleanupOperation::ReconcileV2Target,
            ReconcileOutcome::AbsenceObserved,
            ReconcileReason::AbsenceObserved,
        ) | (
            WorkflowCleanupOperation::ReconcileLegacyTarget,
            ReconcileOutcome::Pending,
            ReconcileReason::WorkflowRunning
                | ReconcileReason::HistoryDeletePending
                | ReconcileReason::VisibilityPending
                | ReconcileReason::TemporalUnavailable,
        ) | (
            WorkflowCleanupOperation::ReconcileV2Target,
            ReconcileOutcome::Pending,
            ReconcileReason::TerminationPending
                | ReconcileReason::HistoryDeletePending
                | ReconcileReason::VisibilityPending
                | ReconcileReason::TemporalUnavailable,
        )
    )
}

fn closed_run_id_set(run_ids: &[String]) -> bool {
    if run_ids.len() > MAX_OBSERVED_RUN_IDS {
        return false;
    }
    let mut previous: Option<&str> = None;
    for run_id in run_ids {
        if !valid_opaque_id(run_id, 128) || previous.is_some_and(|value| value >= run_id.as_str()) {
            return false;
        }
        previous = Some(run_id);
    }
    true
}

fn valid_opaque_id(value: &str, maximum: usize) -> bool {
    (20..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_legacy_workflow_id(value: &str) -> bool {
    const PREFIX: &str = "bluey-jobs:";
    if !(18..=412).contains(&value.len()) || !value.starts_with(PREFIX) || !value.is_ascii() {
        return false;
    }
    let components = &value[PREFIX.len()..];
    if !components
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
    {
        return false;
    }
    components.match_indices(':').any(|(separator, _)| {
        let right_length = components.len().saturating_sub(separator + 1);
        (3..=200).contains(&separator) && (3..=200).contains(&right_length)
    })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn canonical_page_token(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    if value.is_empty() || value.contains('=') {
        return false;
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .filter(|decoded| decoded.len() <= MAX_PAGE_TOKEN_BYTES)
        .is_some_and(|decoded| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(decoded) == value
        })
}

fn gateway_response_headers_are_exact(headers: &HeaderMap) -> bool {
    header_has_exact_single_value(headers, "content-type", b"application/json")
        && header_has_exact_single_value(headers, "cache-control", b"no-store")
        && header_has_exact_single_value(headers, "x-content-type-options", b"nosniff")
}

fn header_has_exact_single_value(headers: &HeaderMap, name: &str, expected: &[u8]) -> bool {
    let mut values = headers.get_all(name).iter();
    values
        .next()
        .is_some_and(|value| value.as_bytes() == expected)
        && values.next().is_none()
}

async fn bounded_response_body(response: reqwest::Response) -> Result<Vec<u8>, ()> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_GATEWAY_BODY_BYTES as u64)
    {
        return Err(());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if body.len().saturating_add(chunk.len()) > MAX_GATEWAY_BODY_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serial_test::serial;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const CLEANUP_ENV_NAMES: &[&str] = &[
        CLEANUP_FLAG,
        ORIGIN_ENV,
        TOKEN_ENV,
        NAMESPACE_ENV,
        VISIBILITY_CUTOFF_ENV,
        POLL_SECONDS_ENV,
        LEASE_MS_ENV,
        CONFIRMATION_MS_ENV,
    ];

    struct CleanupEnvGuard {
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl CleanupEnvGuard {
        fn clean() -> Self {
            let previous = CLEANUP_ENV_NAMES
                .iter()
                .map(|name| (*name, std::env::var_os(name)))
                .collect::<Vec<_>>();
            for name in CLEANUP_ENV_NAMES {
                std::env::remove_var(name);
            }
            Self { previous }
        }
    }

    impl Drop for CleanupEnvGuard {
        fn drop(&mut self) {
            for (name, value) in self.previous.drain(..) {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    fn with_clean_cleanup_env(test: impl FnOnce()) {
        let _guard = CleanupEnvGuard::clean();
        test();
    }

    fn lease_request_identity(lease: &JobsWorkflowCleanupWorkLease) -> (&str, i64, i64, i64) {
        match lease {
            JobsWorkflowCleanupWorkLease::LegacyInventoryPage(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                lease.lease_expires_at_ms,
            ),
            JobsWorkflowCleanupWorkLease::ReconcileLegacyTarget(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                lease.lease_expires_at_ms,
            ),
            JobsWorkflowCleanupWorkLease::ReconcileV2Target(lease) => (
                &lease.cleanup_request_id,
                lease.request_epoch,
                lease.cleanup_fence,
                lease.lease_expires_at_ms,
            ),
        }
    }

    #[test]
    #[serial]
    fn cleanup_disabled_requires_no_gateway_or_inventory_configuration() {
        with_clean_cleanup_env(|| {
            assert!(!cleanup_enabled());
            for disabled in ["0", "1", "TRUE", " true", "yes", "unexpected"] {
                std::env::set_var(CLEANUP_FLAG, disabled);
                assert!(!cleanup_enabled(), "{disabled}");
            }
            std::env::set_var(CLEANUP_FLAG, "true");
            assert!(cleanup_enabled());
        });
    }

    #[test]
    #[serial]
    fn enabled_cleanup_configuration_is_closed_and_bounded() {
        with_clean_cleanup_env(|| {
            std::env::set_var(CLEANUP_FLAG, "1");
            std::env::set_var(ORIGIN_ENV, "https://workflow.example.test");
            std::env::set_var(TOKEN_ENV, "t".repeat(32));
            std::env::set_var(NAMESPACE_ENV, "bluey-jobs-production");
            std::env::set_var(VISIBILITY_CUTOFF_ENV, "1783900800000");
            let config = CleanupDispatcherConfig::from_env().unwrap();
            assert_eq!(config.origin.as_str(), "https://workflow.example.test/");
            assert_eq!(config.namespace, "bluey-jobs-production");
            assert_eq!(config.poll_interval, Duration::from_secs(5));
            assert_eq!(config.lease_ms, DEFAULT_LEASE_MS);
            assert_eq!(config.confirmation_ms, DEFAULT_CONFIRMATION_MS);
            assert_eq!(config.query_digest.len(), 64);

            for (name, value) in [
                (POLL_SECONDS_ENV, "0"),
                (LEASE_MS_ENV, "29999"),
                (CONFIRMATION_MS_ENV, "999"),
                (VISIBILITY_CUTOFF_ENV, "0"),
            ] {
                std::env::set_var(name, value);
                assert!(CleanupDispatcherConfig::from_env().is_err(), "{name}");
                std::env::remove_var(name);
            }
        });
    }

    #[test]
    fn cleanup_query_and_digest_bind_every_fixed_semantic() {
        let cutoff_ms = 1_783_900_800_000;
        let query = legacy_inventory_query(cutoff_ms).unwrap();
        assert_eq!(query, "WorkflowType = \"applicationWorkflow\"");
        let digest = legacy_inventory_query_digest("bluey-jobs", cutoff_ms, &query).unwrap();
        assert_eq!(
            digest,
            "3c9d936edbb8c1f09a56b2278079f97d40731bf0d3d51a939bf2670598a32bf2"
        );
        assert_ne!(
            digest,
            legacy_inventory_query_digest("other-namespace", cutoff_ms, &query).unwrap()
        );
        assert_ne!(
            digest,
            legacy_inventory_query_digest("bluey-jobs", cutoff_ms + 1, &query).unwrap()
        );
    }

    #[test]
    fn cleanup_lease_has_request_and_receipt_persistence_margin() {
        const {
            assert!(HTTP_REQUEST_TIMEOUT_MS > GATEWAY_REQUEST_BUDGET_MS);
            assert!(MIN_LEASE_MS >= HTTP_REQUEST_TIMEOUT_MS + RECEIPT_PERSIST_MARGIN_MS);
        }
    }

    #[test]
    fn v2_target_digest_excludes_the_monotonic_known_run_set() {
        let mut request = ReconcileV2TargetRequest {
            schema_version: 3,
            operation: WorkflowCleanupOperation::ReconcileV2Target,
            cleanup_request_id: format!("wfclean-v3-{}", "4".repeat(32)),
            cleanup_generation_id: format!("wfgeneration-v3-{}", "5".repeat(32)),
            target_set_digest: "6".repeat(64),
            namespace: "bluey-jobs".to_string(),
            workflow_type: "applicationWorkflowV2".to_string(),
            workflow_id: format!("bluey-jobs-v2-{}", "a".repeat(32)),
            first_execution_run_id: Some(format!("temporal-run-{}", "b".repeat(32))),
            start_request_id: format!("wfreq-v2-{}", "e".repeat(32)),
            start_payload_digest: "f".repeat(64),
            known_run_ids: vec![format!("temporal-run-{}", "b".repeat(32))],
            target_digest: "a0367cabb234f15fbc3089323607ae0e245b299cef72d86f6e04b7d42ea82d2b"
                .to_string(),
            cleanup_fence: 11,
            observation_pass: 1,
        };
        assert!(request_target_digest_matches(
            &request,
            V2_TARGET_DIGEST_DOMAIN,
            &request.target_digest,
            &["knownRunIds"],
        ));

        request
            .known_run_ids
            .push(format!("temporal-run-{}", "c".repeat(32)));
        assert!(request_target_digest_matches(
            &request,
            V2_TARGET_DIGEST_DOMAIN,
            &request.target_digest,
            &["knownRunIds"],
        ));
    }

    #[test]
    fn v2_first_run_and_known_run_set_bind_atomically() {
        let bound = v2_reconcile_request_fixture(Some(format!("temporal-run-{}", "b".repeat(32))));
        let config = static_cleanup_config_fixture(&bound.namespace);
        assert!(outbound_request_is_valid(
            &config,
            &WorkflowCleanupRequest::ReconcileV2Target(bound)
        ));

        let mut unbound = v2_reconcile_request_fixture(None);
        assert!(outbound_request_is_valid(
            &config,
            &WorkflowCleanupRequest::ReconcileV2Target(unbound.clone())
        ));
        unbound
            .known_run_ids
            .push(format!("temporal-run-{}", "b".repeat(32)));
        assert!(!outbound_request_is_valid(
            &config,
            &WorkflowCleanupRequest::ReconcileV2Target(unbound)
        ));
    }

    #[test]
    fn known_run_set_is_bounded_to_32_for_the_gateway_deadline() {
        let max = (0..MAX_OBSERVED_RUN_IDS)
            .map(|index| format!("run-{index:03}-{}", "a".repeat(120)))
            .collect::<Vec<_>>();
        assert_eq!(max.len(), 32);
        assert!(closed_run_id_set(&max));
        assert!(max.iter().all(|run_id| run_id.len() == 128));

        let mut request = v2_reconcile_request_fixture(Some(max[0].clone()));
        request.known_run_ids = max.clone();
        request.target_digest =
            request_target_digest(&request, V2_TARGET_DIGEST_DOMAIN, &["knownRunIds"]);
        let receipt = v2_reconcile_receipt_fixture(
            &request,
            ReconcileOutcome::AbsenceObserved,
            ReconcileReason::AbsenceObserved,
            request.first_execution_run_id.clone(),
            max.clone(),
        );
        assert!(serde_json::to_vec(&receipt).unwrap().len() <= MAX_GATEWAY_BODY_BYTES);

        let mut oversized = max;
        oversized.push(format!(
            "run-{:03}-{}",
            MAX_OBSERVED_RUN_IDS,
            "a".repeat(120)
        ));
        assert_eq!(oversized.len(), 33);
        assert!(!closed_run_id_set(&oversized));
    }

    #[test]
    fn cleanup_gateway_token_matches_the_private_gateway_grammar() {
        assert!(valid_gateway_token(&"x".repeat(32)));
        assert!(valid_gateway_token(&format!("{}==", "x".repeat(32))));
        assert!(!valid_gateway_token(&"x".repeat(31)));
        assert!(!valid_gateway_token(&format!(
            "{} internal",
            "x".repeat(32)
        )));
        assert!(!valid_gateway_token(&format!("{}\nsecond", "x".repeat(32))));
        assert!(!valid_gateway_token(&format!(
            "{}={}",
            "x".repeat(16),
            "x".repeat(16)
        )));
    }

    #[test]
    fn legacy_workflow_ids_accept_the_historical_colon_envelope_only() {
        assert!(valid_legacy_workflow_id(
            "bluey-jobs:550e8400-e29b-41d4-a716-446655440000:apply_company_role_123"
        ));
        assert!(valid_legacy_workflow_id("bluey-jobs:abc:def"));
        assert!(valid_legacy_workflow_id("bluey-jobs:abc:def:ghi"));
        assert!(valid_legacy_workflow_id(&format!(
            "bluey-jobs:{}:{}",
            "a".repeat(200),
            "b".repeat(200)
        )));

        for invalid in [
            "bluey-jobs:ab:def".to_string(),
            "bluey-jobs:abc:de".to_string(),
            "bluey-jobs:abc:def\"".to_string(),
            "bluey-jobs:abc:def\\".to_string(),
            "other-jobs:abc:def".to_string(),
            format!("bluey-jobs:{}:def", "a".repeat(201)),
            format!("bluey-jobs:abc:{}", "b".repeat(201)),
        ] {
            assert!(!valid_legacy_workflow_id(&invalid), "{invalid}");
        }
    }

    #[test]
    fn cleanup_gateway_origin_rejects_non_https_and_ambient_url_state() {
        assert!(validated_gateway_origin("https://user@example.com").is_err());
        assert!(validated_gateway_origin("https://example.com/base").is_err());
        assert!(validated_gateway_origin("https://example.com?token=secret").is_err());
        assert!(validated_gateway_origin("http://example.com").is_err());
    }

    #[tokio::test]
    async fn cleanup_gateway_client_never_follows_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/target", server.uri())),
            )
            .expect(1)
            .mount(&server)
            .await;
        let response = gateway_client()
            .unwrap()
            .get(format!("{}/redirect", server.uri()))
            .bearer_auth("must-not-cross-redirect")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn cleanup_dispatch_posts_only_the_exact_inventory_authority() {
        let server = MockServer::start().await;
        let request = inventory_request_fixture();
        let receipt = inventory_receipt_fixture(&request);
        Mock::given(method("POST"))
            .and(path("/workflow-cleanup"))
            .respond_with(
                ResponseTemplate::new(202)
                    .insert_header("content-type", "application/json")
                    .insert_header("cache-control", "no-store")
                    .insert_header("x-content-type-options", "nosniff")
                    .set_body_json(receipt.clone()),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = cleanup_config_fixture(&server);
        let classified = deliver_cleanup_request(
            &gateway_client().unwrap(),
            &config,
            &WorkflowCleanupRequest::LegacyInventoryPage(request.clone()),
        )
        .await;
        assert_eq!(classified, ClassifiedGatewayResponse::Accepted(receipt));
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body,
            json!({
                "schemaVersion": 3,
                "operation": "legacy_inventory_page",
                "cleanupRequestId": request.cleanup_request_id,
                "inventoryGenerationId": request.inventory_generation_id,
                "namespace": request.namespace,
                "workflowType": "applicationWorkflow",
                "visibilityCutoffMs": request.visibility_cutoff_ms,
                "queryDigest": request.query_digest,
                "scanPass": 1,
                "pageIndex": 0,
                "predecessorPageDigest": null,
                "pageToken": null,
                "cleanupFence": 1,
            })
        );
    }

    #[tokio::test]
    #[serial]
    async fn cleanup_dispatch_persists_request_start_before_io_and_records_receipt() {
        let _env = CleanupEnvGuard::clean();
        std::env::set_var(CLEANUP_FLAG, "true");
        let pool = crate::db::open_pool(":memory:".as_ref()).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let server = MockServer::start().await;
        let config = cleanup_config_fixture(&server);
        let now_ms = jobs::now_ms();
        let prepared = jobs::prepare_jobs_legacy_inventory_generation(
            &pool,
            &jobs::PrepareJobsLegacyInventoryGeneration {
                namespace: config.namespace.clone(),
                visibility_cutoff_ms: config.visibility_cutoff_ms,
                confirmation_age_ms: config.confirmation_ms,
                now_ms,
            },
        )
        .unwrap();
        let lease = jobs::claim_jobs_workflow_cleanup_work(
            &pool,
            "cleanup-dispatch-test-owner",
            now_ms + 1,
            config.lease_ms,
        )
        .unwrap()
        .expect("claim initial inventory page");
        let JobsWorkflowCleanupWorkLease::LegacyInventoryPage(page) = &lease else {
            panic!("initial cleanup claim must be the global inventory page");
        };
        let request = legacy_inventory_request_for_lease(page);
        let receipt = inventory_receipt_fixture(&request);
        let request_started_before_http = Arc::new(AtomicBool::new(false));
        let observed_start = Arc::clone(&request_started_before_http);
        let observed_pool = pool.clone();
        let observed_generation = prepared.authority.inventory_generation_id.clone();
        Mock::given(method("POST"))
            .and(path("/workflow-cleanup"))
            .and(header(
                "authorization",
                "Bearer cleanup-token-123456789012345678",
            ))
            .respond_with(move |_request: &wiremock::Request| {
                let started: i64 = observed_pool
                    .get()
                    .unwrap()
                    .query_row(
                        "SELECT COUNT(*)
                           FROM jobs_workflow_legacy_inventory_generations
                          WHERE inventory_generation_id = ?1
                            AND first_request_started_at_ms IS NOT NULL",
                        rusqlite::params![observed_generation],
                        |row| row.get(0),
                    )
                    .unwrap();
                observed_start.store(started == 1, Ordering::SeqCst);
                ResponseTemplate::new(202)
                    .insert_header("content-type", "application/json")
                    .insert_header("cache-control", "no-store")
                    .insert_header("x-content-type-options", "nosniff")
                    .set_body_json(receipt.clone())
            })
            .expect(1)
            .mount(&server)
            .await;

        process_cleanup_work(&pool, &gateway_client().unwrap(), &config, &lease)
            .await
            .unwrap();
        assert!(request_started_before_http.load(Ordering::SeqCst));
        let stored: (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state,
                        (SELECT COUNT(*) FROM jobs_workflow_legacy_targets target
                          WHERE target.generation = generation.generation)
                   FROM jobs_workflow_legacy_inventory_generations generation
                  WHERE inventory_generation_id = ?1",
                rusqlite::params![prepared.authority.inventory_generation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, ("draining".to_string(), 1));
    }

    #[tokio::test]
    #[serial]
    async fn cleanup_dispatch_retries_gateway_outage_with_stable_request_identity() {
        let _env = CleanupEnvGuard::clean();
        std::env::set_var(CLEANUP_FLAG, "true");
        let pool = crate::db::open_pool(":memory:".as_ref()).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let server = MockServer::start().await;
        let config = cleanup_config_fixture(&server);
        let now_ms = jobs::now_ms();
        jobs::prepare_jobs_legacy_inventory_generation(
            &pool,
            &jobs::PrepareJobsLegacyInventoryGeneration {
                namespace: config.namespace.clone(),
                visibility_cutoff_ms: config.visibility_cutoff_ms,
                confirmation_age_ms: config.confirmation_ms,
                now_ms,
            },
        )
        .unwrap();
        let first = jobs::claim_jobs_workflow_cleanup_work(
            &pool,
            "cleanup-retry-test-owner",
            now_ms + 1,
            config.lease_ms,
        )
        .unwrap()
        .expect("claim initial inventory page");
        let (first_request_id, first_epoch, first_fence, _) = lease_request_identity(&first);
        let first_request_id = first_request_id.to_string();
        Mock::given(method("POST"))
            .and(path("/workflow-cleanup"))
            .respond_with(
                ResponseTemplate::new(503)
                    .insert_header("content-type", "application/json")
                    .insert_header("cache-control", "no-store")
                    .insert_header("x-content-type-options", "nosniff")
                    .set_body_json(json!({
                        "schemaVersion": 3,
                        "outcome": "rejected",
                        "reason": "temporal_unavailable"
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;

        process_cleanup_work(&pool, &gateway_client().unwrap(), &config, &first)
            .await
            .unwrap();
        assert!(jobs::claim_jobs_workflow_cleanup_work(
            &pool,
            "cleanup-retry-test-owner",
            jobs::now_ms() + 6_000,
            config.lease_ms,
        )
        .unwrap()
        .is_none());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(7);
        let retry = loop {
            if let Some(retry) = jobs::claim_jobs_workflow_cleanup_work(
                &pool,
                "cleanup-retry-test-owner",
                jobs::now_ms(),
                config.lease_ms,
            )
            .unwrap()
            {
                break retry;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "gateway outage did not become retryable on the database clock"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let (retry_request_id, retry_epoch, retry_fence, _) = lease_request_identity(&retry);
        assert_eq!(retry_request_id, first_request_id.as_str());
        assert_eq!(retry_epoch, first_epoch);
        assert_eq!(retry_fence, first_fence);
    }

    #[test]
    fn cleanup_receipt_after_lease_expiry_cannot_advance_inventory() {
        let pool = crate::db::open_pool(":memory:".as_ref()).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let now_ms = 1_000_000;
        jobs::prepare_jobs_legacy_inventory_generation(
            &pool,
            &jobs::PrepareJobsLegacyInventoryGeneration {
                namespace: "bluey-jobs-expired-receipt-test".to_string(),
                visibility_cutoff_ms: 900_000,
                confirmation_age_ms: 1_000,
                now_ms,
            },
        )
        .unwrap();
        let lease = jobs::claim_jobs_workflow_cleanup_work(
            &pool,
            "cleanup-expired-receipt-owner",
            now_ms + 1,
            1_000,
        )
        .unwrap()
        .expect("claim inventory page");
        let JobsWorkflowCleanupWorkLease::LegacyInventoryPage(page) = &lease else {
            panic!("initial cleanup claim must be an inventory page");
        };
        let request = legacy_inventory_request_for_lease(page);
        let receipt = inventory_receipt_fixture(&request);
        jobs::mark_jobs_workflow_cleanup_request_started(&pool, &lease, now_ms + 2).unwrap();
        let expires_at_ms = lease_request_identity(&lease).3;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let database_now_ms: i64 = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            if database_now_ms > expires_at_ms {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cleanup lease did not expire on the database clock"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            jobs::record_jobs_workflow_cleanup_receipt(&pool, &lease, &receipt, now_ms + 2,)
                .is_err()
        );
        let page_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_workflow_legacy_inventory_pages",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(page_count, 0);
    }

    #[test]
    fn cleanup_lease_mapping_omits_private_database_lease_authority() {
        let request = inventory_request_fixture();
        let lease =
            JobsWorkflowCleanupWorkLease::LegacyInventoryPage(JobsLegacyInventoryPageLease {
                cleanup_request_id: request.cleanup_request_id.clone(),
                inventory_generation_id: request.inventory_generation_id.clone(),
                namespace: request.namespace.clone(),
                workflow_type: request.workflow_type.clone(),
                visibility_cutoff_ms: request.visibility_cutoff_ms,
                query_digest: request.query_digest.clone(),
                scan_pass: request.scan_pass,
                page_index: request.page_index,
                predecessor_page_digest: request.predecessor_page_digest.clone(),
                page_token: request.page_token.clone(),
                cleanup_fence: request.cleanup_fence,
                request_epoch: 7,
                lease_owner: "cleanup-owner-1234567890".to_string(),
                lease_token: "cleanup-lease-token-1234567890".to_string(),
                lease_expires_at_ms: 99_000,
            });
        assert!(cleanup_lease_has_delivery_budget(&lease, 69_000));
        assert!(!cleanup_lease_has_delivery_budget(&lease, 71_001));
        let outbound = serde_json::to_value(cleanup_request_for_lease(&lease)).unwrap();
        assert_eq!(outbound, serde_json::to_value(request).unwrap());
        for private in [
            "requestEpoch",
            "leaseOwner",
            "leaseToken",
            "leaseExpiresAtMs",
        ] {
            assert!(outbound.get(private).is_none(), "{private}");
        }
    }

    #[test]
    fn cleanup_page_receipt_requires_exact_echo_chain_and_closed_targets() {
        let request = inventory_request_fixture();
        let exact = inventory_receipt_fixture(&request);
        assert!(matches!(
            classify_gateway_response(
                &WorkflowCleanupRequest::LegacyInventoryPage(request.clone()),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&exact).unwrap(),
            ),
            ClassifiedGatewayResponse::Accepted(_)
        ));
        for invalid in [
            {
                let mut invalid = exact.clone();
                invalid["cleanupFence"] = json!(2);
                invalid
            },
            {
                let mut invalid = exact.clone();
                invalid["nextPageToken"] = json!("not+base64");
                invalid["exhausted"] = json!(false);
                invalid
            },
            {
                let mut invalid = exact.clone();
                invalid["targets"][0]["status"] = json!("UNKNOWN");
                invalid
            },
            {
                let mut invalid = exact;
                invalid["extra"] = json!(true);
                invalid
            },
        ] {
            assert_eq!(
                classify_gateway_response(
                    &WorkflowCleanupRequest::LegacyInventoryPage(request.clone()),
                    StatusCode::ACCEPTED,
                    true,
                    &serde_json::to_vec(&invalid).unwrap(),
                ),
                ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
            );
        }
    }

    #[test]
    fn cleanup_reconcile_receipt_requires_exact_target_and_proof_epoch() {
        let request = legacy_reconcile_request_fixture();
        let mut exact = json!({
            "schemaVersion": 3,
            "operation": "reconcile_legacy_target",
            "cleanupRequestId": request.cleanup_request_id,
            "inventoryGenerationId": request.inventory_generation_id,
            "namespace": request.namespace,
            "workflowType": "applicationWorkflow",
            "visibilityCutoffMs": request.visibility_cutoff_ms,
            "queryDigest": request.query_digest,
            "scanPass": request.scan_pass,
            "workflowId": request.workflow_id,
            "runId": request.run_id,
            "targetDigest": request.target_digest,
            "cleanupFence": request.cleanup_fence,
            "observationPass": request.observation_pass,
            "outcome": "absence_observed",
            "reason": "absence_observed",
            "firstExecutionRunId": request.first_execution_run_id,
            "runIds": [request.run_id],
        });
        let evidence_digest =
            cleanup_domain_digest(CLEANUP_EVIDENCE_DIGEST_DOMAIN, &exact).unwrap();
        exact["evidenceDigest"] = json!(evidence_digest);
        assert!(matches!(
            classify_gateway_response(
                &WorkflowCleanupRequest::ReconcileLegacyTarget(request.clone()),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&exact).unwrap(),
            ),
            ClassifiedGatewayResponse::Accepted(_)
        ));
        let mut overbound = exact.clone();
        overbound["runIds"] = json!([request.run_id.clone(), "legacy-run-extra-1234567890"]);
        overbound.as_object_mut().unwrap().remove("evidenceDigest");
        let overbound_digest =
            cleanup_domain_digest(CLEANUP_EVIDENCE_DIGEST_DOMAIN, &overbound).unwrap();
        overbound["evidenceDigest"] = json!(overbound_digest);
        assert_eq!(
            classify_gateway_response(
                &WorkflowCleanupRequest::ReconcileLegacyTarget(request.clone()),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&overbound).unwrap(),
            ),
            ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
        );
        let mut stale = exact;
        stale["observationPass"] = json!(2);
        assert_eq!(
            classify_gateway_response(
                &WorkflowCleanupRequest::ReconcileLegacyTarget(request),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&stale).unwrap(),
            ),
            ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
        );
    }

    #[test]
    fn v2_receipt_can_only_discover_new_identity_as_visibility_pending() {
        let request =
            v2_reconcile_request_fixture(Some(format!("temporal-run-{}", "b".repeat(32))));
        let request_envelope = WorkflowCleanupRequest::ReconcileV2Target(request.clone());
        let second_run = format!("temporal-run-{}", "c".repeat(32));

        let discovered = v2_reconcile_receipt_fixture(
            &request,
            ReconcileOutcome::Pending,
            ReconcileReason::VisibilityPending,
            request.first_execution_run_id.clone(),
            vec![request.known_run_ids[0].clone(), second_run.clone()],
        );
        assert!(matches!(
            classify_gateway_response(
                &request_envelope,
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&discovered).unwrap(),
            ),
            ClassifiedGatewayResponse::Accepted(_)
        ));

        for (outcome, reason) in [
            (
                ReconcileOutcome::AbsenceObserved,
                ReconcileReason::AbsenceObserved,
            ),
            (
                ReconcileOutcome::Pending,
                ReconcileReason::HistoryDeletePending,
            ),
            (
                ReconcileOutcome::Pending,
                ReconcileReason::TemporalUnavailable,
            ),
        ] {
            let invalid = v2_reconcile_receipt_fixture(
                &request,
                outcome,
                reason,
                request.first_execution_run_id.clone(),
                vec![request.known_run_ids[0].clone(), second_run.clone()],
            );
            assert_eq!(
                classify_gateway_response(
                    &request_envelope,
                    StatusCode::ACCEPTED,
                    true,
                    &serde_json::to_vec(&invalid).unwrap(),
                ),
                ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
            );
        }

        let unbound = v2_reconcile_request_fixture(None);
        let newly_bound = v2_reconcile_receipt_fixture(
            &unbound,
            ReconcileOutcome::Pending,
            ReconcileReason::VisibilityPending,
            Some(second_run.clone()),
            vec![second_run.clone()],
        );
        assert!(matches!(
            classify_gateway_response(
                &WorkflowCleanupRequest::ReconcileV2Target(unbound.clone()),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&newly_bound).unwrap(),
            ),
            ClassifiedGatewayResponse::Accepted(_)
        ));
        for (first_execution_run_id, run_ids) in [
            (None, vec![second_run.clone()]),
            (Some(second_run.clone()), Vec::new()),
        ] {
            let split_identity = v2_reconcile_receipt_fixture(
                &unbound,
                ReconcileOutcome::Pending,
                ReconcileReason::VisibilityPending,
                first_execution_run_id,
                run_ids,
            );
            assert_eq!(
                classify_gateway_response(
                    &WorkflowCleanupRequest::ReconcileV2Target(unbound.clone()),
                    StatusCode::ACCEPTED,
                    true,
                    &serde_json::to_vec(&split_identity).unwrap(),
                ),
                ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
            );
        }
        let invalid = v2_reconcile_receipt_fixture(
            &unbound,
            ReconcileOutcome::AbsenceObserved,
            ReconcileReason::AbsenceObserved,
            Some(second_run.clone()),
            vec![second_run],
        );
        assert_eq!(
            classify_gateway_response(
                &WorkflowCleanupRequest::ReconcileV2Target(unbound),
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&invalid).unwrap(),
            ),
            ClassifiedGatewayResponse::Retryable(RetryableGatewayReason::MalformedResponse)
        );
    }

    #[test]
    fn reconcile_pending_reasons_are_operation_specific() {
        assert!(reconcile_outcome_reason_is_valid(
            WorkflowCleanupOperation::ReconcileLegacyTarget,
            ReconcileOutcome::Pending,
            ReconcileReason::WorkflowRunning,
        ));
        assert!(!reconcile_outcome_reason_is_valid(
            WorkflowCleanupOperation::ReconcileLegacyTarget,
            ReconcileOutcome::Pending,
            ReconcileReason::TerminationPending,
        ));
        assert!(reconcile_outcome_reason_is_valid(
            WorkflowCleanupOperation::ReconcileV2Target,
            ReconcileOutcome::Pending,
            ReconcileReason::TerminationPending,
        ));
        assert!(!reconcile_outcome_reason_is_valid(
            WorkflowCleanupOperation::ReconcileV2Target,
            ReconcileOutcome::Pending,
            ReconcileReason::WorkflowRunning,
        ));
    }

    #[test]
    fn only_exact_closed_v3_client_errors_are_permanent() {
        let request = WorkflowCleanupRequest::LegacyInventoryPage(inventory_request_fixture());
        for (status, body, expected) in [
            (
                StatusCode::BAD_REQUEST,
                json!({"schemaVersion":3,"outcome":"rejected","reason":"invalid_request"}),
                ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::InvalidRequest),
            ),
            (
                StatusCode::NOT_FOUND,
                json!({"schemaVersion":3,"outcome":"rejected","reason":"not_found"}),
                ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::NotFound),
            ),
            (
                StatusCode::CONFLICT,
                json!({
                    "schemaVersion": 3,
                    "outcome": "identity_conflict",
                    "reason": "identity_conflict",
                }),
                ClassifiedGatewayResponse::Rejected(PermanentGatewayRejection::IdentityConflict),
            ),
        ] {
            assert_eq!(
                classify_gateway_response(
                    &request,
                    status,
                    true,
                    &serde_json::to_vec(&body).unwrap(),
                ),
                expected
            );
        }

        for (status, body) in [
            (
                StatusCode::UNAUTHORIZED,
                json!({"schemaVersion":2,"outcome":"rejected","reason":"invalid_request"}),
            ),
            (
                StatusCode::NOT_FOUND,
                json!({"schemaVersion":2,"outcome":"rejected","reason":"invalid_request"}),
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"schemaVersion":3,"outcome":"rejected","reason":"temporal_unavailable"}),
            ),
        ] {
            assert!(matches!(
                classify_gateway_response(
                    &request,
                    status,
                    true,
                    &serde_json::to_vec(&body).unwrap(),
                ),
                ClassifiedGatewayResponse::Retryable(_)
            ));
        }
    }

    #[test]
    fn cleanup_response_headers_are_closed_and_exact() {
        let exact = exact_gateway_response_headers();
        assert!(gateway_response_headers_are_exact(&exact));
        for name in ["content-type", "cache-control", "x-content-type-options"] {
            let mut missing = exact.clone();
            missing.remove(name);
            assert!(!gateway_response_headers_are_exact(&missing));
        }
        let mut wrong = exact;
        wrong.insert(
            "content-type",
            reqwest::header::HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(!gateway_response_headers_are_exact(&wrong));
    }

    #[test]
    fn page_tokens_are_canonical_unpadded_base64url_and_bounded() {
        assert!(canonical_page_token(None));
        assert!(canonical_page_token(Some("AQID")));
        assert!(!canonical_page_token(Some("")));
        assert!(!canonical_page_token(Some("AQID=")));
        assert!(!canonical_page_token(Some("AQ+ID")));
        let oversized = vec![0_u8; MAX_PAGE_TOKEN_BYTES + 1];
        let oversized = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(oversized);
        assert!(!canonical_page_token(Some(&oversized)));
    }

    #[test]
    fn inventory_page_index_is_bounded_to_4096_pages_per_pass() {
        let config = static_cleanup_config_fixture("bluey-jobs-test");
        let mut request = inventory_request_fixture();
        request.page_index = MAX_PAGE_INDEX;
        request.predecessor_page_digest = Some("a".repeat(64));
        request.page_token = Some("AQID".to_string());
        assert_eq!(MAX_LEGACY_INVENTORY_PAGES_PER_PASS, 4_096);
        assert!(outbound_request_is_valid(
            &config,
            &WorkflowCleanupRequest::LegacyInventoryPage(request.clone()),
        ));
        let mut receipt: LegacyInventoryPageReceipt =
            serde_json::from_value(inventory_receipt_fixture(&request)).unwrap();
        assert!(inventory_receipt_matches(&request, &receipt));
        receipt.next_page_token = Some("AQID".to_string());
        receipt.exhausted = false;
        assert!(!inventory_receipt_matches(&request, &receipt));

        request.page_index = MAX_PAGE_INDEX + 1;
        assert!(!outbound_request_is_valid(
            &config,
            &WorkflowCleanupRequest::LegacyInventoryPage(request.clone()),
        ));
        let receipt: LegacyInventoryPageReceipt =
            serde_json::from_value(inventory_receipt_fixture(&request)).unwrap();
        assert!(!inventory_receipt_matches(&request, &receipt));
    }

    fn cleanup_config_fixture(server: &MockServer) -> CleanupDispatcherConfig {
        let visibility_cutoff_ms = 1_783_900_800_000;
        let namespace = "bluey-jobs-test".to_string();
        let query = legacy_inventory_query(visibility_cutoff_ms).unwrap();
        let query_digest =
            legacy_inventory_query_digest(&namespace, visibility_cutoff_ms, &query).unwrap();
        CleanupDispatcherConfig {
            origin: validated_gateway_origin(&server.uri()).unwrap(),
            token: "cleanup-token-123456789012345678".to_string(),
            namespace,
            visibility_cutoff_ms,
            query,
            query_digest,
            poll_interval: Duration::from_secs(1),
            lease_ms: DEFAULT_LEASE_MS,
            confirmation_ms: DEFAULT_CONFIRMATION_MS,
        }
    }

    fn static_cleanup_config_fixture(namespace: &str) -> CleanupDispatcherConfig {
        let visibility_cutoff_ms = 1_783_900_800_000;
        let query = legacy_inventory_query(visibility_cutoff_ms).unwrap();
        CleanupDispatcherConfig {
            origin: validated_gateway_origin("https://workflow.example.test").unwrap(),
            token: "cleanup-token-123456789012345678".to_string(),
            namespace: namespace.to_string(),
            visibility_cutoff_ms,
            query_digest: legacy_inventory_query_digest(namespace, visibility_cutoff_ms, &query)
                .unwrap(),
            query,
            poll_interval: Duration::from_secs(1),
            lease_ms: DEFAULT_LEASE_MS,
            confirmation_ms: DEFAULT_CONFIRMATION_MS,
        }
    }

    fn inventory_request_fixture() -> LegacyInventoryPageRequest {
        let visibility_cutoff_ms = 1_783_900_800_000;
        let namespace = "bluey-jobs-test".to_string();
        let query = legacy_inventory_query(visibility_cutoff_ms).unwrap();
        LegacyInventoryPageRequest {
            schema_version: 3,
            operation: WorkflowCleanupOperation::LegacyInventoryPage,
            cleanup_request_id: "wfcleanupreq-v3-1234567890".to_string(),
            inventory_generation_id: "wfinventory-v3-1234567890".to_string(),
            namespace: namespace.clone(),
            workflow_type: LEGACY_WORKFLOW_TYPE.to_string(),
            visibility_cutoff_ms,
            query_digest: legacy_inventory_query_digest(&namespace, visibility_cutoff_ms, &query)
                .unwrap(),
            scan_pass: 1,
            page_index: 0,
            predecessor_page_digest: None,
            page_token: None,
            cleanup_fence: 1,
        }
    }

    fn inventory_receipt_fixture(request: &LegacyInventoryPageRequest) -> Value {
        let targets = json!([{
            "workflowId": "bluey-jobs:account_123456:idempotency_123456",
            "runId": "legacy-run-123456789012345",
            "firstExecutionRunId": "legacy-first-run-1234567890",
            "status": "COMPLETED",
        }]);
        let targets_digest = cleanup_domain_digest(LEGACY_TARGETS_DIGEST_DOMAIN, &targets).unwrap();
        let mut receipt = json!({
            "schemaVersion": 3,
            "operation": "legacy_inventory_page",
            "cleanupRequestId": request.cleanup_request_id,
            "inventoryGenerationId": request.inventory_generation_id,
            "namespace": request.namespace,
            "workflowType": request.workflow_type,
            "visibilityCutoffMs": request.visibility_cutoff_ms,
            "queryDigest": request.query_digest,
            "scanPass": request.scan_pass,
            "pageIndex": request.page_index,
            "predecessorPageDigest": request.predecessor_page_digest,
            "pageToken": request.page_token,
            "cleanupFence": request.cleanup_fence,
            "outcome": "page",
            "targetsDigest": targets_digest,
            "targets": targets,
            "nextPageToken": null,
            "exhausted": true,
        });
        let page_digest = cleanup_domain_digest(LEGACY_PAGE_DIGEST_DOMAIN, &receipt).unwrap();
        receipt["pageDigest"] = json!(page_digest);
        receipt
    }

    fn legacy_reconcile_request_fixture() -> ReconcileLegacyTargetRequest {
        let inventory = inventory_request_fixture();
        let mut request = ReconcileLegacyTargetRequest {
            schema_version: 3,
            operation: WorkflowCleanupOperation::ReconcileLegacyTarget,
            cleanup_request_id: "wfcleanuptarget-v3-1234567890".to_string(),
            inventory_generation_id: inventory.inventory_generation_id,
            namespace: inventory.namespace,
            workflow_type: LEGACY_WORKFLOW_TYPE.to_string(),
            visibility_cutoff_ms: inventory.visibility_cutoff_ms,
            query_digest: inventory.query_digest,
            scan_pass: 1,
            workflow_id: "bluey-jobs:account_123456:idempotency_123456".to_string(),
            run_id: "legacy-run-123456789012345".to_string(),
            first_execution_run_id: "legacy-first-run-1234567890".to_string(),
            target_digest: "0".repeat(64),
            cleanup_fence: 1,
            observation_pass: 1,
        };
        request.target_digest = request_target_digest(&request, LEGACY_TARGET_DIGEST_DOMAIN, &[]);
        request
    }

    fn v2_reconcile_request_fixture(
        first_execution_run_id: Option<String>,
    ) -> ReconcileV2TargetRequest {
        let mut request = ReconcileV2TargetRequest {
            schema_version: 3,
            operation: WorkflowCleanupOperation::ReconcileV2Target,
            cleanup_request_id: format!("wfclean-v3-{}", "4".repeat(32)),
            cleanup_generation_id: format!("wfgeneration-v3-{}", "5".repeat(32)),
            target_set_digest: "6".repeat(64),
            namespace: "bluey-jobs-test".to_string(),
            workflow_type: "applicationWorkflowV2".to_string(),
            workflow_id: format!("bluey-jobs-v2-{}", "a".repeat(32)),
            known_run_ids: first_execution_run_id.iter().cloned().collect(),
            first_execution_run_id,
            start_request_id: format!("wfreq-v2-{}", "e".repeat(32)),
            start_payload_digest: "f".repeat(64),
            target_digest: "0".repeat(64),
            cleanup_fence: 11,
            observation_pass: 1,
        };
        request.target_digest =
            request_target_digest(&request, V2_TARGET_DIGEST_DOMAIN, &["knownRunIds"]);
        request
    }

    fn v2_reconcile_receipt_fixture(
        request: &ReconcileV2TargetRequest,
        outcome: ReconcileOutcome,
        reason: ReconcileReason,
        first_execution_run_id: Option<String>,
        run_ids: Vec<String>,
    ) -> Value {
        let mut receipt = json!({
            "schemaVersion": request.schema_version,
            "operation": request.operation,
            "cleanupRequestId": request.cleanup_request_id,
            "cleanupGenerationId": request.cleanup_generation_id,
            "targetSetDigest": request.target_set_digest,
            "namespace": request.namespace,
            "workflowType": request.workflow_type,
            "workflowId": request.workflow_id,
            "firstExecutionRunId": first_execution_run_id,
            "startRequestId": request.start_request_id,
            "startPayloadDigest": request.start_payload_digest,
            "knownRunIds": request.known_run_ids,
            "targetDigest": request.target_digest,
            "cleanupFence": request.cleanup_fence,
            "observationPass": request.observation_pass,
            "outcome": outcome,
            "reason": reason,
            "runIds": run_ids,
        });
        let evidence_digest =
            cleanup_domain_digest(CLEANUP_EVIDENCE_DIGEST_DOMAIN, &receipt).unwrap();
        receipt["evidenceDigest"] = json!(evidence_digest);
        receipt
    }

    #[test]
    fn cleanup_configuration_is_validated_before_background_workers_start() {
        for source in [
            include_str!("main.rs"),
            include_str!("bin/bluey-jobs-api.rs"),
        ] {
            let cleanup = source
                .find("spawn_jobs_workflow_cleanup_dispatcher")
                .expect("cleanup dispatcher startup must remain registered");
            for worker in [
                "spawn_expired_usage_reservation_janitor",
                "spawn_spend_truth_janitor",
                "spawn_mailbox_sync_worker",
                "spawn_communication_workers",
                "spawn_jobs_workflow_command_dispatcher",
            ] {
                let worker_start = source
                    .find(worker)
                    .expect("background worker must remain registered");
                assert!(
                    cleanup < worker_start,
                    "cleanup configuration must be validated before {worker}"
                );
            }
            for optional_worker in [
                "spawn_cleanup_worker",
                "spawn_global_candidate_archive_worker",
            ] {
                if let Some(worker_start) = source.find(optional_worker) {
                    assert!(
                        cleanup < worker_start,
                        "cleanup configuration must be validated before {optional_worker}"
                    );
                }
            }
        }
    }

    fn request_target_digest<T: Serialize>(
        request: &T,
        domain: &[u8],
        additional_mutable_fields: &[&str],
    ) -> String {
        let mut value = serde_json::to_value(request).unwrap();
        let object = value.as_object_mut().unwrap();
        for key in [
            "schemaVersion",
            "operation",
            "cleanupRequestId",
            "targetDigest",
            "cleanupFence",
            "observationPass",
        ] {
            object.remove(key).unwrap();
        }
        for key in additional_mutable_fields {
            object.remove(*key).unwrap();
        }
        cleanup_domain_digest(domain, &value).unwrap()
    }

    fn exact_gateway_response_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "cache-control",
            reqwest::header::HeaderValue::from_static("no-store"),
        );
        headers.insert(
            "x-content-type-options",
            reqwest::header::HeaderValue::from_static("nosniff"),
        );
        headers
    }
}
