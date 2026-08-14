//! Disabled-by-default delivery for durable Jobs workflow commands.
//!
//! API handlers only commit commands. This worker is the sole network boundary
//! to the Temporal gateway and records request-start evidence before any I/O.

use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use reqwest::{header::HeaderMap, redirect::Policy, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::db::{
    jobs::{
        self, JobsWorkflowAcceptanceReceipt, JobsWorkflowAcceptedOutcome, JobsWorkflowCommand,
        JobsWorkflowCommandCompletion, JobsWorkflowCommandKind, JobsWorkflowCommandLease,
        JobsWorkflowRejectionReason, JobsWorkflowUnknownReason,
    },
    DbPool,
};

const DISPATCH_FLAG: &str = "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED";
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
const DEFAULT_LEASE_MS: i64 = 30_000;
const CLAIM_BATCH_SIZE: usize = 25;
const MAX_GATEWAY_BODY_BYTES: usize = 16 * 1024;
const RETRY_BASE_MS: i64 = 5_000;
const RETRY_MAX_MS: i64 = 5 * 60_000;

#[derive(Clone)]
struct DispatcherConfig {
    origin: Url,
    token: String,
    poll_interval: Duration,
    lease_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowGatewayCommand<'a> {
    schema_version: i64,
    operation: WorkflowOperation,
    request_id: &'a str,
    workflow_id: &'a str,
    payload_digest: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    intervention_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowOperation {
    Start,
    Resume,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowGatewayAcceptedOutcome {
    Accepted,
    AlreadyAccepted,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowGatewayReceipt {
    schema_version: i64,
    outcome: WorkflowGatewayAcceptedOutcome,
    request_id: String,
    workflow_id: String,
    payload_digest: String,
    temporal_run_id: String,
    #[serde(default, deserialize_with = "deserialize_present_intervention_id")]
    intervention_id: Option<String>,
}

fn deserialize_present_intervention_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowGatewayErrorOutcome {
    IdentityConflict,
    Rejected,
    DeliveryUnknown,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkflowGatewayErrorReason {
    IdentityConflict,
    InvalidRequest,
    UnsupportedProtocol,
    WorkflowNotFound,
    WorkflowClosed,
    DescribeAmbiguous,
    TemporalUnavailable,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowGatewayError {
    schema_version: i64,
    outcome: WorkflowGatewayErrorOutcome,
    reason: WorkflowGatewayErrorReason,
}

/// Starts the command dispatcher only when its separate release flag is set.
///
/// Enabling the worker with incomplete or unsafe configuration fails startup;
/// the ordinary disabled state does not require gateway credentials.
pub fn spawn_jobs_workflow_command_dispatcher(
    pool: DbPool,
) -> anyhow::Result<Option<JoinHandle<()>>> {
    if !dispatch_enabled() {
        tracing::info!("Jobs workflow command dispatcher disabled");
        return Ok(None);
    }
    let config = DispatcherConfig::from_env()?;
    let client = gateway_client()?;
    let owner = format!("jobs-workflow-dispatch-{}", uuid::Uuid::new_v4());
    Ok(Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if !dispatch_enabled() {
                continue;
            }
            if run_dispatch_cycle(&pool, &client, &config, &owner)
                .await
                .is_err()
            {
                tracing::warn!(
                    reason_code = "workflow_dispatch_cycle_database_failed",
                    "Jobs workflow command dispatch cycle did not complete"
                );
            }
        }
    })))
}

impl DispatcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        let origin = std::env::var("BLUEY_JOBS_WORKFLOW_ORIGIN")
            .context("BLUEY_JOBS_WORKFLOW_ORIGIN is required when workflow dispatch is enabled")?;
        let origin = validated_gateway_origin(&origin)?;
        let token = std::env::var("BLUEY_JOBS_WORKFLOW_TOKEN")
            .unwrap_or_default()
            .trim()
            .to_string();
        anyhow::ensure!(
            valid_gateway_token(&token),
            "BLUEY_JOBS_WORKFLOW_TOKEN is invalid when workflow dispatch is enabled"
        );
        let poll_interval = std::env::var("BLUEY_JOBS_WORKFLOW_COMMAND_POLL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS)
            .clamp(1, 300);
        let lease_ms = std::env::var("BLUEY_JOBS_WORKFLOW_COMMAND_LEASE_MS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(DEFAULT_LEASE_MS)
            .clamp(20_000, 5 * 60_000);
        Ok(Self {
            origin,
            token,
            poll_interval: Duration::from_secs(poll_interval),
            lease_ms,
        })
    }
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

fn dispatch_enabled() -> bool {
    std::env::var(DISPATCH_FLAG)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn validated_gateway_origin(raw: &str) -> anyhow::Result<Url> {
    let mut origin = Url::parse(raw.trim()).context("parse BLUEY_JOBS_WORKFLOW_ORIGIN")?;
    anyhow::ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "BLUEY_JOBS_WORKFLOW_ORIGIN must not contain credentials, query, or fragment"
    );
    let local_debug_origin = cfg!(debug_assertions)
        && origin.scheme() == "http"
        && matches!(origin.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    anyhow::ensure!(
        origin.scheme() == "https" || local_debug_origin,
        "BLUEY_JOBS_WORKFLOW_ORIGIN must use HTTPS"
    );
    anyhow::ensure!(
        matches!(origin.path(), "" | "/"),
        "BLUEY_JOBS_WORKFLOW_ORIGIN must not contain a path"
    );
    origin.set_path("/");
    Ok(origin)
}

fn gateway_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(Policy::none())
        .user_agent("bluey-jobs-workflow-command/2")
        .build()
        .context("build Jobs workflow command gateway client")
}

async fn run_dispatch_cycle(
    pool: &DbPool,
    client: &reqwest::Client,
    config: &DispatcherConfig,
    owner: &str,
) -> anyhow::Result<()> {
    for _ in 0..CLAIM_BATCH_SIZE {
        if !dispatch_enabled() {
            break;
        }
        let now_ms = jobs::now_ms();
        let Some(lease) = jobs::claim_jobs_workflow_command(pool, owner, now_ms, config.lease_ms)?
        else {
            break;
        };
        if let Err(reason_code) = process_command(pool, client, config, &lease).await {
            tracing::warn!(
                reason_code,
                "Jobs workflow command delivery did not complete"
            );
        }
    }
    Ok(())
}

async fn process_command(
    pool: &DbPool,
    client: &reqwest::Client,
    config: &DispatcherConfig,
    lease: &JobsWorkflowCommandLease,
) -> Result<(), &'static str> {
    if !dispatch_enabled() {
        return Ok(());
    }
    let now_ms = jobs::now_ms();
    if lease.lease_expires_at_ms <= now_ms {
        return Err("workflow_command_lease_expired_before_request");
    }
    jobs::mark_jobs_workflow_command_request_started(pool, lease, now_ms)
        .map_err(|_| "workflow_command_request_start_evidence_failed")?;

    let completion = deliver_command(client, config, &lease.command).await;
    let completed_at_ms = jobs::now_ms();
    let retry_at_ms = matches!(
        completion,
        JobsWorkflowCommandCompletion::DeliveryUnknown(_)
    )
    .then(|| workflow_command_retry_at(&lease.command, completed_at_ms));
    jobs::complete_jobs_workflow_command(pool, lease, completion, completed_at_ms, retry_at_ms)
        .map(|_| ())
        .map_err(|_| "workflow_command_completion_evidence_failed")
}

async fn deliver_command(
    client: &reqwest::Client,
    config: &DispatcherConfig,
    command: &JobsWorkflowCommand,
) -> JobsWorkflowCommandCompletion {
    let operation = match command.command_kind {
        JobsWorkflowCommandKind::Start => WorkflowOperation::Start,
        JobsWorkflowCommandKind::Resume => WorkflowOperation::Resume,
    };
    let outbound = WorkflowGatewayCommand {
        schema_version: 2,
        operation,
        request_id: &command.request_id,
        workflow_id: &command.workflow_id,
        payload_digest: &command.payload_hmac_sha256,
        intervention_id: command.intervention_id.as_deref(),
    };
    let endpoint = match config.origin.join("workflow-commands") {
        Ok(endpoint) => endpoint,
        Err(_) => {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::GatewayUnavailable,
            )
        }
    };
    let response = match client
        .post(endpoint)
        .bearer_auth(&config.token)
        .json(&outbound)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::TransportTimeout,
            )
        }
        Err(error) if error.is_connect() => {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::ConnectionLost,
            )
        }
        Err(_) => {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::GatewayUnavailable,
            )
        }
    };
    let status = response.status();
    let response_headers_are_exact = gateway_response_headers_are_exact(response.headers());
    let body = match bounded_response_body(response).await {
        Ok(body) => body,
        Err(()) => {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::MalformedResponse,
            )
        }
    };
    classify_gateway_response(command, status, response_headers_are_exact, &body)
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

fn classify_gateway_response(
    command: &JobsWorkflowCommand,
    status: StatusCode,
    response_headers_are_exact: bool,
    body: &[u8],
) -> JobsWorkflowCommandCompletion {
    if !response_headers_are_exact {
        return JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::MalformedResponse,
        );
    }
    if status == StatusCode::ACCEPTED {
        let Ok(receipt) = serde_json::from_slice::<WorkflowGatewayReceipt>(body) else {
            return JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::MalformedResponse,
            );
        };
        if !receipt_matches(command, &receipt) {
            return JobsWorkflowCommandCompletion::IdentityConflict;
        }
        let outcome = match receipt.outcome {
            WorkflowGatewayAcceptedOutcome::Accepted => JobsWorkflowAcceptedOutcome::Accepted,
            WorkflowGatewayAcceptedOutcome::AlreadyAccepted => {
                JobsWorkflowAcceptedOutcome::AlreadyAccepted
            }
        };
        return JobsWorkflowCommandCompletion::Accepted(JobsWorkflowAcceptanceReceipt {
            outcome,
            request_id: receipt.request_id,
            payload_hmac_sha256: receipt.payload_digest,
            workflow_id: receipt.workflow_id,
            temporal_run_id: receipt.temporal_run_id,
            intervention_id: receipt.intervention_id,
        });
    }

    if status.is_success() {
        return JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::MalformedResponse,
        );
    }

    let parsed = serde_json::from_slice::<WorkflowGatewayError>(body).ok();
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return JobsWorkflowCommandCompletion::DeliveryUnknown(
            if status == StatusCode::GATEWAY_TIMEOUT {
                JobsWorkflowUnknownReason::TransportTimeout
            } else if status == StatusCode::SERVICE_UNAVAILABLE {
                JobsWorkflowUnknownReason::GatewayUnavailable
            } else {
                JobsWorkflowUnknownReason::GatewayServerError
            },
        );
    }
    let Some(error) = parsed.filter(|error| error.schema_version == 2) else {
        return JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::MalformedResponse,
        );
    };
    match (error.outcome, error.reason) {
        (WorkflowGatewayErrorOutcome::Rejected, WorkflowGatewayErrorReason::InvalidRequest)
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) =>
        {
            JobsWorkflowCommandCompletion::Rejected(JobsWorkflowRejectionReason::Unauthorized)
        }
        (
            WorkflowGatewayErrorOutcome::IdentityConflict,
            WorkflowGatewayErrorReason::IdentityConflict,
        ) if status == StatusCode::CONFLICT => JobsWorkflowCommandCompletion::IdentityConflict,
        (WorkflowGatewayErrorOutcome::Rejected, WorkflowGatewayErrorReason::InvalidRequest)
            if status == StatusCode::BAD_REQUEST =>
        {
            JobsWorkflowCommandCompletion::Rejected(JobsWorkflowRejectionReason::InvalidRequest)
        }
        (
            WorkflowGatewayErrorOutcome::Rejected,
            WorkflowGatewayErrorReason::UnsupportedProtocol,
        ) if status == StatusCode::BAD_REQUEST => JobsWorkflowCommandCompletion::Rejected(
            JobsWorkflowRejectionReason::UnsupportedProtocol,
        ),
        (WorkflowGatewayErrorOutcome::Rejected, WorkflowGatewayErrorReason::WorkflowNotFound)
            if status == StatusCode::NOT_FOUND =>
        {
            JobsWorkflowCommandCompletion::Rejected(JobsWorkflowRejectionReason::WorkflowNotFound)
        }
        (WorkflowGatewayErrorOutcome::Rejected, WorkflowGatewayErrorReason::WorkflowClosed)
            if status == StatusCode::CONFLICT =>
        {
            JobsWorkflowCommandCompletion::Rejected(JobsWorkflowRejectionReason::GatewayRejected)
        }
        (
            WorkflowGatewayErrorOutcome::DeliveryUnknown,
            WorkflowGatewayErrorReason::DescribeAmbiguous,
        ) => JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::MalformedResponse,
        ),
        (
            WorkflowGatewayErrorOutcome::DeliveryUnknown,
            WorkflowGatewayErrorReason::TemporalUnavailable,
        ) => JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::GatewayUnavailable,
        ),
        _ => JobsWorkflowCommandCompletion::DeliveryUnknown(
            JobsWorkflowUnknownReason::MalformedResponse,
        ),
    }
}

fn receipt_matches(command: &JobsWorkflowCommand, receipt: &WorkflowGatewayReceipt) -> bool {
    let intervention_matches = match command.command_kind {
        JobsWorkflowCommandKind::Start => receipt.intervention_id.is_none(),
        JobsWorkflowCommandKind::Resume => receipt.intervention_id == command.intervention_id,
    };
    receipt.schema_version == 2
        && receipt.request_id == command.request_id
        && receipt.workflow_id == command.workflow_id
        && receipt.payload_digest == command.payload_hmac_sha256
        && valid_temporal_run_id(&receipt.temporal_run_id)
        && intervention_matches
}

fn valid_temporal_run_id(value: &str) -> bool {
    (20..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn workflow_command_retry_at(command: &JobsWorkflowCommand, now_ms: i64) -> i64 {
    let exponent =
        u32::try_from(command.attempt_count.saturating_sub(1).clamp(0, 6)).unwrap_or_default();
    let delay_ms = RETRY_BASE_MS
        .saturating_mul(2_i64.saturating_pow(exponent))
        .min(RETRY_MAX_MS);
    now_ms.saturating_add(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn gateway_client_never_follows_redirects() {
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
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dispatcher_posts_only_the_closed_v2_authority_to_the_exact_route() {
        let server = MockServer::start().await;
        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        Mock::given(method("POST"))
            .and(path("/workflow-commands"))
            .respond_with(
                ResponseTemplate::new(202)
                    .insert_header("content-type", "application/json")
                    .insert_header("cache-control", "no-store")
                    .insert_header("x-content-type-options", "nosniff")
                    .set_body_json(json!({
                        "schemaVersion": 2,
                        "outcome": "accepted",
                        "requestId": command.request_id,
                        "workflowId": command.workflow_id,
                        "payloadDigest": command.payload_hmac_sha256,
                        "temporalRunId": "temporal-run-1234567890",
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = DispatcherConfig {
            origin: validated_gateway_origin(&server.uri()).unwrap(),
            token: "gateway-token-12345678901234567890".to_string(),
            poll_interval: Duration::from_secs(1),
            lease_ms: DEFAULT_LEASE_MS,
        };
        assert!(matches!(
            deliver_command(&gateway_client().unwrap(), &config, &command).await,
            JobsWorkflowCommandCompletion::Accepted(_)
        ));
        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body,
            json!({
                "schemaVersion": 2,
                "operation": "start",
                "requestId": command.request_id,
                "workflowId": command.workflow_id,
                "payloadDigest": command.payload_hmac_sha256,
            })
        );
    }

    #[tokio::test]
    async fn dispatcher_timeout_is_delivery_unknown() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workflow-commands"))
            .respond_with(ResponseTemplate::new(202).set_delay(Duration::from_millis(250)))
            .mount(&server)
            .await;
        let config = DispatcherConfig {
            origin: validated_gateway_origin(&server.uri()).unwrap(),
            token: "gateway-token-12345678901234567890".to_string(),
            poll_interval: Duration::from_secs(1),
            lease_ms: DEFAULT_LEASE_MS,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(25))
            .redirect(Policy::none())
            .build()
            .unwrap();
        assert_eq!(
            deliver_command(
                &client,
                &config,
                &command_fixture(JobsWorkflowCommandKind::Start, None),
            )
            .await,
            JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::TransportTimeout
            )
        );
    }

    #[test]
    fn dispatcher_flag_is_disabled_when_missing_or_invalid() {
        let previous = std::env::var_os(DISPATCH_FLAG);
        std::env::remove_var(DISPATCH_FLAG);
        assert!(!dispatch_enabled());
        std::env::set_var(DISPATCH_FLAG, "unexpected");
        assert!(!dispatch_enabled());
        match previous {
            Some(value) => std::env::set_var(DISPATCH_FLAG, value),
            None => std::env::remove_var(DISPATCH_FLAG),
        }
    }

    #[test]
    fn gateway_token_uses_the_same_header_safe_grammar_as_the_gateway() {
        assert!(valid_gateway_token(&"x".repeat(32)));
        assert!(valid_gateway_token(&format!("{}==", "x".repeat(32))));
        assert!(!valid_gateway_token(&"x".repeat(31)));
        assert!(!valid_gateway_token(&format!(
            "{} internal",
            "x".repeat(32)
        )));
        assert!(!valid_gateway_token(&format!("{}\nsecond", "x".repeat(32))));
        assert!(!valid_gateway_token(&format!("{}é", "x".repeat(31))));
        assert!(!valid_gateway_token(&format!(
            "{}={}",
            "x".repeat(16),
            "x".repeat(16)
        )));
    }

    #[test]
    fn production_gateway_origin_rejects_paths_and_credentials() {
        assert!(validated_gateway_origin("https://user@example.com").is_err());
        assert!(validated_gateway_origin("https://example.com/base").is_err());
        assert!(validated_gateway_origin("https://example.com?token=secret").is_err());
        assert_eq!(
            validated_gateway_origin("https://workflow.example.com")
                .unwrap()
                .as_str(),
            "https://workflow.example.com/"
        );
    }

    #[test]
    fn accepted_receipt_requires_every_exact_authority_field() {
        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        let exact = json!({
            "schemaVersion": 2,
            "outcome": "accepted",
            "requestId": command.request_id,
            "workflowId": command.workflow_id,
            "payloadDigest": command.payload_hmac_sha256,
            "temporalRunId": "temporal-run-1234567890",
        });
        assert!(matches!(
            classify_gateway_response(
                &command,
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&exact).unwrap(),
            ),
            JobsWorkflowCommandCompletion::Accepted(JobsWorkflowAcceptanceReceipt {
                outcome: JobsWorkflowAcceptedOutcome::Accepted,
                ..
            })
        ));

        let mut mismatch = exact;
        mismatch["workflowId"] = json!("different-workflow-1234567890");
        assert_eq!(
            classify_gateway_response(
                &command,
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&mismatch).unwrap(),
            ),
            JobsWorkflowCommandCompletion::IdentityConflict
        );
    }

    #[test]
    fn resume_receipt_requires_the_exact_intervention() {
        let command = command_fixture(
            JobsWorkflowCommandKind::Resume,
            Some("intervention-1234567890"),
        );
        let missing_intervention = json!({
            "schemaVersion": 2,
            "outcome": "already_accepted",
            "requestId": command.request_id,
            "workflowId": command.workflow_id,
            "payloadDigest": command.payload_hmac_sha256,
            "temporalRunId": "temporal-run-1234567890",
        });
        assert_eq!(
            classify_gateway_response(
                &command,
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&missing_intervention).unwrap(),
            ),
            JobsWorkflowCommandCompletion::IdentityConflict
        );

        let mut null_intervention = missing_intervention.clone();
        null_intervention["interventionId"] = serde_json::Value::Null;
        assert_eq!(
            classify_gateway_response(
                &command,
                StatusCode::ACCEPTED,
                true,
                &serde_json::to_vec(&null_intervention).unwrap(),
            ),
            JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::MalformedResponse
            )
        );
    }

    #[test]
    fn accepted_receipt_rejects_null_and_extra_fields() {
        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        let receipt = json!({
            "schemaVersion": 2,
            "outcome": "accepted",
            "requestId": command.request_id,
            "workflowId": command.workflow_id,
            "payloadDigest": command.payload_hmac_sha256,
            "temporalRunId": "temporal-run-1234567890",
        });
        for invalid in [
            {
                let mut invalid = receipt.clone();
                invalid["interventionId"] = serde_json::Value::Null;
                invalid
            },
            {
                let mut invalid = receipt;
                invalid["extra"] = json!(true);
                invalid
            },
        ] {
            assert_eq!(
                classify_gateway_response(
                    &command,
                    StatusCode::ACCEPTED,
                    true,
                    &serde_json::to_vec(&invalid).unwrap(),
                ),
                JobsWorkflowCommandCompletion::DeliveryUnknown(
                    JobsWorkflowUnknownReason::MalformedResponse
                )
            );
        }
    }

    #[test]
    fn malformed_success_and_generic_conflict_remain_ambiguous() {
        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        let wrong_success_status = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "accepted",
            "requestId": command.request_id,
            "workflowId": command.workflow_id,
            "payloadDigest": command.payload_hmac_sha256,
            "temporalRunId": "temporal-run-1234567890",
        }))
        .unwrap();
        for (status, body) in [
            (StatusCode::ACCEPTED, br#"{"accepted":true}"#.as_slice()),
            (StatusCode::OK, wrong_success_status.as_slice()),
            (StatusCode::NO_CONTENT, b"".as_slice()),
            (
                StatusCode::CONFLICT,
                br#"{"error":"already exists"}"#.as_slice(),
            ),
        ] {
            assert_eq!(
                classify_gateway_response(&command, status, true, body),
                JobsWorkflowCommandCompletion::DeliveryUnknown(
                    JobsWorkflowUnknownReason::MalformedResponse
                )
            );
        }
    }

    #[test]
    fn gateway_response_headers_are_closed_and_exact() {
        let exact = exact_gateway_response_headers();
        assert!(gateway_response_headers_are_exact(&exact));

        for name in ["content-type", "cache-control", "x-content-type-options"] {
            let mut missing = exact.clone();
            missing.remove(name);
            assert!(!gateway_response_headers_are_exact(&missing));
        }

        let mut wrong_content_type = exact.clone();
        wrong_content_type.insert(
            "content-type",
            reqwest::header::HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(!gateway_response_headers_are_exact(&wrong_content_type));

        let mut duplicate = exact;
        duplicate.append(
            "cache-control",
            reqwest::header::HeaderValue::from_static("no-store"),
        );
        assert!(!gateway_response_headers_are_exact(&duplicate));

        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        assert_eq!(
            classify_gateway_response(&command, StatusCode::ACCEPTED, false, br#"{}"#),
            JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::MalformedResponse
            )
        );
    }

    #[test]
    fn temporal_run_id_matches_the_gateway_opaque_identifier_contract() {
        assert!(valid_temporal_run_id("temporal-run-1234567890"));
        for invalid in [
            "short",
            "temporal.run.1234567890",
            "temporal:run:1234567890",
        ] {
            assert!(!valid_temporal_run_id(invalid));
        }
        assert!(!valid_temporal_run_id(&"a".repeat(129)));
    }

    #[test]
    fn closed_gateway_errors_map_only_from_the_exact_schema() {
        let command = command_fixture(JobsWorkflowCommandKind::Start, None);
        let identity_conflict = json!({
            "schemaVersion": 2,
            "outcome": "identity_conflict",
            "reason": "identity_conflict",
        });
        assert_eq!(
            classify_gateway_response(
                &command,
                StatusCode::CONFLICT,
                true,
                &serde_json::to_vec(&identity_conflict).unwrap(),
            ),
            JobsWorkflowCommandCompletion::IdentityConflict
        );

        let unavailable = json!({
            "schemaVersion": 2,
            "outcome": "delivery_unknown",
            "reason": "temporal_unavailable",
        });
        assert_eq!(
            classify_gateway_response(
                &command,
                StatusCode::SERVICE_UNAVAILABLE,
                true,
                &serde_json::to_vec(&unavailable).unwrap(),
            ),
            JobsWorkflowCommandCompletion::DeliveryUnknown(
                JobsWorkflowUnknownReason::GatewayUnavailable
            )
        );
    }

    fn command_fixture(
        command_kind: JobsWorkflowCommandKind,
        intervention_id: Option<&str>,
    ) -> JobsWorkflowCommand {
        let request_id = "wfreq-v2-12345678901234567890".to_string();
        let workflow_id = "bluey-jobs-v2-12345678901234567890".to_string();
        let payload_hmac_sha256 = "a".repeat(64);
        let payload = match command_kind {
            JobsWorkflowCommandKind::Start => {
                jobs::JobsWorkflowCommandPayload::Start(jobs::JobsWorkflowStartMaterial {
                    workflow_input: json!({}),
                    browser_session_id: "cloud-application-1234567890".to_string(),
                    result_request_id: request_id.clone(),
                })
            }
            JobsWorkflowCommandKind::Resume => {
                jobs::JobsWorkflowCommandPayload::Resume(jobs::JobsWorkflowResumeMaterial {
                    workflow_input: json!({}),
                    browser_session_id: "cloud-application-1234567890".to_string(),
                    result_request_id: request_id.clone(),
                    resolution: json!({ "action": "approve_submission" }),
                })
            }
        };
        JobsWorkflowCommand {
            id: "wfcmd-v2-12345678901234567890".to_string(),
            account_id: "account-1234567890".to_string(),
            application_id: "application-1234567890".to_string(),
            run_id: "run-12345678901234567890".to_string(),
            workflow_id: workflow_id.clone(),
            intervention_id: intervention_id.map(str::to_string),
            command_kind,
            protocol_version: 2,
            idempotency_key_hmac_sha256: "b".repeat(64),
            request_id: request_id.clone(),
            request_hmac_sha256: "c".repeat(64),
            payload_hmac_sha256: payload_hmac_sha256.clone(),
            state: jobs::JobsWorkflowCommandState::Delivering,
            envelope: jobs::JobsWorkflowCommandEnvelope {
                schema_version: 2,
                command_kind,
                request_id,
                request_hmac_sha256: "c".repeat(64),
                payload_hmac_sha256,
                workflow_id,
                application_id: "application-1234567890".to_string(),
                run_id: "run-12345678901234567890".to_string(),
                intervention_id: intervention_id.map(str::to_string),
                payload,
            },
            attempt_count: 1,
            fence: 1,
            lease_owner: Some("owner-1234567890".to_string()),
            lease_expires_at_ms: Some(60_000),
            active_attempt_id: Some("attempt-1234567890".to_string()),
            first_request_started_at_ms: Some(1),
            first_ambiguous_at_ms: None,
            next_attempt_at_ms: None,
            last_outcome_code: None,
            temporal_run_id: None,
            accepted_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 1,
        }
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
