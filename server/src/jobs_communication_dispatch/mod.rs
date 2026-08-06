//! Disabled-by-default provider execution and reconciliation for reviewed Jobs communication.

mod contracts;
mod providers;

use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::{
    db::{
        jobs::{
            self, JobsCommunicationActionFinish, JobsCommunicationActionLease,
            JobsCommunicationActionReconciliation, JobsCommunicationLeaseAccess,
            JobsProviderCredential, MailboxConnection,
        },
        DbPool,
    },
    jobs_provider_auth::{
        action_provider_is_granted, action_provider_lookup_is_granted, env_flag_enabled,
        normalized_scopes, provider_config, refresh_access_token, ProviderAuthErrorKind,
        ProviderAuthorizationPurpose,
    },
};

use contracts::{
    CommunicationProviderRequest, CommunicationSourceMessage, ProviderCommitEvidence,
    ProviderDispatchResult, ProviderFailureKind, ProviderLookupResult,
};
use providers::ProviderEndpoints;

const DISPATCH_FLAG: &str = "BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED";
const RECONCILIATION_FLAG: &str = "BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED";
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 30;
const CLAIM_BATCH_SIZE: usize = 25;
const REFRESH_SKEW_MS: i64 = 60_000;
const AUTHORITATIVE_ABSENCE_AGE_MS: i64 = 15 * 60_000;

pub struct CommunicationWorkerHandles {
    dispatch: Option<JoinHandle<()>>,
    reconciliation: Option<JoinHandle<()>>,
}

impl CommunicationWorkerHandles {
    pub fn abort(self) {
        if let Some(worker) = self.dispatch {
            worker.abort();
        }
        if let Some(worker) = self.reconciliation {
            worker.abort();
        }
    }
}

pub fn spawn_communication_workers(pool: DbPool) -> CommunicationWorkerHandles {
    let poll_interval = std::env::var("BLUEY_JOBS_COMMUNICATION_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS)
        .clamp(5, 300);
    let dispatch = env_flag_enabled(DISPATCH_FLAG).then(|| {
        let pool = pool.clone();
        let owner = format!("jobs-communication-dispatch-{}", uuid::Uuid::new_v4());
        tokio::spawn(async move {
            let Some(client) = provider_client() else {
                tracing::error!(
                    reason_code = "provider_client_configuration_failed",
                    "Jobs communication dispatch worker stopped"
                );
                return;
            };
            let endpoints = ProviderEndpoints::production();
            let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if !env_flag_enabled(DISPATCH_FLAG) {
                    continue;
                }
                if run_dispatch_cycle(&pool, &client, &endpoints, &owner)
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        reason_code = "dispatch_cycle_database_failed",
                        "Jobs communication dispatch cycle did not complete"
                    );
                }
            }
        })
    });
    if dispatch.is_none() {
        tracing::info!("Jobs communication dispatch worker disabled");
    }

    let reconciliation = env_flag_enabled(RECONCILIATION_FLAG).then(|| {
        let owner = format!("jobs-communication-reconcile-{}", uuid::Uuid::new_v4());
        tokio::spawn(async move {
            let Some(client) = provider_client() else {
                tracing::error!(
                    reason_code = "provider_client_configuration_failed",
                    "Jobs communication reconciliation worker stopped"
                );
                return;
            };
            let endpoints = ProviderEndpoints::production();
            let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if !env_flag_enabled(RECONCILIATION_FLAG) {
                    continue;
                }
                if run_reconciliation_cycle(&pool, &client, &endpoints, &owner)
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        reason_code = "reconciliation_cycle_database_failed",
                        "Jobs communication reconciliation cycle did not complete"
                    );
                }
            }
        })
    });
    if reconciliation.is_none() {
        tracing::info!("Jobs communication reconciliation worker disabled");
    }

    CommunicationWorkerHandles {
        dispatch,
        reconciliation,
    }
}

fn provider_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("bluey-jobs-communication/1")
        .build()
        .ok()
}

async fn run_dispatch_cycle(
    pool: &DbPool,
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    owner: &str,
) -> anyhow::Result<()> {
    for _ in 0..CLAIM_BATCH_SIZE {
        if !env_flag_enabled(DISPATCH_FLAG) {
            break;
        }
        let Some(lease) = jobs::claim_communication_action(pool, owner)? else {
            break;
        };
        if let Err(reason_code) = process_dispatch(pool, client, endpoints, owner, &lease).await {
            tracing::warn!(
                action_id = %lease.action.id,
                provider = %lease.action.provider,
                reason_code,
                "Jobs communication dispatch did not complete"
            );
        }
    }
    Ok(())
}

async fn process_dispatch(
    pool: &DbPool,
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    owner: &str,
    lease: &JobsCommunicationActionLease,
) -> Result<(), &'static str> {
    let access = lease_access(lease, owner);
    if !env_flag_enabled(DISPATCH_FLAG) {
        finish_no_side_effect(pool, access, "dispatch_release_gate_closed", false)?;
        return Ok(());
    }
    let credential = match prepare_credential(pool, client, lease, CredentialUse::Dispatch).await {
        Ok(credential) => credential,
        Err(failure) => {
            finish_no_side_effect(pool, access, failure.reason_code, failure.authorization)?;
            if failure.authorization {
                let _ = jobs::mark_mailbox_reauthorization_required(
                    pool,
                    &lease.account_id,
                    &lease.action.connection_id,
                );
            }
            return Ok(());
        }
    };
    let request = match provider_request(pool, lease) {
        Ok(request) => request,
        Err(reason_code) => {
            finish_no_side_effect(pool, access, reason_code, false)?;
            return Ok(());
        }
    };

    jobs::mark_communication_action_request_started(pool, &access)
        .map_err(|_| "request_start_evidence_failed")?;
    let result =
        dispatch_before_lease_deadline(lease.action.lease_expires_at_ms, jobs::now_ms(), || {
            providers::dispatch(client, endpoints, &credential.access_token, &request)
        })
        .await?;
    let reauthorization_after_finish = matches!(
        &result,
        ProviderDispatchResult::DefinitiveNoSideEffect {
            kind: ProviderFailureKind::Authorization,
            ..
        }
    );
    let finish = match result {
        ProviderDispatchResult::Committed(evidence) => JobsCommunicationActionFinish {
            lease: access,
            outcome: if lease.action.kind == "reply" {
                "sent".to_string()
            } else {
                "calendar_created".to_string()
            },
            provider_object_id: evidence.provider_object_id,
            evidence: bound_evidence(&request, evidence.evidence),
        },
        ProviderDispatchResult::DefinitiveNoSideEffect {
            kind,
            retry_after_ms,
        } => JobsCommunicationActionFinish {
            lease: access,
            outcome: "needs_input".to_string(),
            provider_object_id: String::new(),
            evidence: json!({
                "schema_version": 1,
                "reason_code": kind.code(),
                "no_side_effect": true,
                "authorization_required": kind == ProviderFailureKind::Authorization,
                "provider_retry_after_ms": retry_after_ms,
            }),
        },
        ProviderDispatchResult::Ambiguous { reason_code } => JobsCommunicationActionFinish {
            lease: access,
            outcome: "side_effect_unknown".to_string(),
            provider_object_id: String::new(),
            evidence: json!({
                "schema_version": 1,
                "reason_code": reason_code,
                "provider_outcome": "unknown",
            }),
        },
    };
    jobs::finish_communication_action(pool, &finish)
        .map(|_| ())
        .map_err(|_| "dispatch_completion_evidence_failed")?;
    if reauthorization_after_finish {
        let _ = jobs::mark_mailbox_reauthorization_required(
            pool,
            &lease.account_id,
            &lease.action.connection_id,
        );
    }
    Ok(())
}

async fn dispatch_before_lease_deadline<F, Fut>(
    lease_expires_at_ms: Option<i64>,
    now_ms: i64,
    dispatch: F,
) -> Result<ProviderDispatchResult, &'static str>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ProviderDispatchResult>,
{
    let remaining_ms = lease_expires_at_ms
        .ok_or("dispatch_lease_deadline_missing")?
        .checked_sub(now_ms)
        .filter(|remaining| *remaining > 0)
        .ok_or("dispatch_lease_deadline_expired")?;
    let remaining_ms =
        u64::try_from(remaining_ms).map_err(|_| "dispatch_lease_deadline_invalid")?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(remaining_ms))
        .ok_or("dispatch_lease_deadline_invalid")?;
    tokio::time::timeout_at(deadline, dispatch())
        .await
        .map_err(|_| "dispatch_lease_deadline_elapsed")
}

fn finish_no_side_effect(
    pool: &DbPool,
    access: JobsCommunicationLeaseAccess,
    reason_code: &'static str,
    authorization_required: bool,
) -> Result<(), &'static str> {
    jobs::finish_communication_action(
        pool,
        &JobsCommunicationActionFinish {
            lease: access,
            outcome: "needs_input".to_string(),
            provider_object_id: String::new(),
            evidence: json!({
                "schema_version": 1,
                "reason_code": reason_code,
                "no_side_effect": true,
                "authorization_required": authorization_required,
            }),
        },
    )
    .map(|_| ())
    .map_err(|_| "pre_request_completion_evidence_failed")
}

async fn run_reconciliation_cycle(
    pool: &DbPool,
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    owner: &str,
) -> anyhow::Result<()> {
    for _ in 0..CLAIM_BATCH_SIZE {
        if !env_flag_enabled(RECONCILIATION_FLAG) {
            break;
        }
        let Some(lease) = jobs::claim_communication_action_reconciliation(pool, owner)? else {
            break;
        };
        if let Err(reason_code) =
            process_reconciliation(pool, client, endpoints, owner, &lease).await
        {
            tracing::warn!(
                action_id = %lease.action.id,
                provider = %lease.action.provider,
                reason_code,
                "Jobs communication reconciliation did not complete"
            );
        }
    }
    Ok(())
}

async fn process_reconciliation(
    pool: &DbPool,
    client: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    owner: &str,
    lease: &JobsCommunicationActionLease,
) -> Result<(), &'static str> {
    let access = lease_access(lease, owner);
    if !env_flag_enabled(RECONCILIATION_FLAG) {
        return finish_inconclusive(pool, lease, owner, "reconciliation_release_gate_closed");
    }
    let credential = match prepare_credential(pool, client, lease, CredentialUse::Lookup).await {
        Ok(credential) => credential,
        Err(failure) => {
            finish_inconclusive(pool, lease, owner, failure.reason_code)?;
            if failure.authorization {
                let _ = jobs::mark_mailbox_reauthorization_required(
                    pool,
                    &lease.account_id,
                    &lease.action.connection_id,
                );
            }
            return Ok(());
        }
    };
    let request = match provider_request(pool, lease) {
        Ok(request) => request,
        Err(reason_code) => return finish_inconclusive(pool, lease, owner, reason_code),
    };
    let lookup = providers::lookup(client, endpoints, &credential.access_token, &request).await;
    let reauthorization_after_finish = matches!(
        &lookup,
        ProviderLookupResult::Inconclusive {
            reason_code: "provider_lookup_authorization_required",
        }
    );
    let reconciliation = match lookup {
        ProviderLookupResult::Found(ProviderCommitEvidence {
            provider_object_id,
            evidence,
        }) => JobsCommunicationActionReconciliation {
            lease: access,
            resolution: if lease.action.kind == "reply" {
                "confirmed_sent".to_string()
            } else {
                "confirmed_calendar".to_string()
            },
            provider_object_id,
            evidence: bound_evidence(&request, evidence),
        },
        // The database atomically counts only persisted confirmed-absence observations for this
        // exact attempt. Inconclusive/crashed claims do not advance the three-observation bound.
        ProviderLookupResult::Absent if absence_age_ready(lease) => {
            JobsCommunicationActionReconciliation {
                lease: access,
                resolution: "confirmed_absent".to_string(),
                provider_object_id: String::new(),
                evidence: bound_reconciliation_evidence(
                    lease,
                    json!({
                        "schema_version": 1,
                        "authoritative_absence": true,
                        "provider_lookup": "exact_absence",
                        "minimum_age_ms": AUTHORITATIVE_ABSENCE_AGE_MS,
                    }),
                ),
            }
        }
        ProviderLookupResult::Absent => JobsCommunicationActionReconciliation {
            lease: access,
            resolution: "inconclusive".to_string(),
            provider_object_id: String::new(),
            evidence: bound_reconciliation_evidence(
                lease,
                json!({
                    "schema_version": 1,
                    "reason_code": "provider_absence_age_floor_pending",
                    "minimum_age_ms": AUTHORITATIVE_ABSENCE_AGE_MS,
                }),
            ),
        },
        ProviderLookupResult::Inconclusive { reason_code } => {
            JobsCommunicationActionReconciliation {
                lease: access,
                resolution: "inconclusive".to_string(),
                provider_object_id: String::new(),
                evidence: bound_reconciliation_evidence(
                    lease,
                    json!({
                        "schema_version": 1,
                        "reason_code": reason_code,
                    }),
                ),
            }
        }
        ProviderLookupResult::Conflict => JobsCommunicationActionReconciliation {
            lease: access,
            resolution: "inconclusive".to_string(),
            provider_object_id: String::new(),
            evidence: bound_reconciliation_evidence(
                lease,
                json!({
                    "schema_version": 1,
                    "reason_code": "provider_lookup_conflict",
                }),
            ),
        },
    };
    jobs::reconcile_communication_action(pool, &reconciliation)
        .map(|_| ())
        .map_err(|_| "reconciliation_evidence_failed")?;
    if reauthorization_after_finish {
        let _ = jobs::mark_mailbox_reauthorization_required(
            pool,
            &lease.account_id,
            &lease.action.connection_id,
        );
    }
    Ok(())
}

fn finish_inconclusive(
    pool: &DbPool,
    lease: &JobsCommunicationActionLease,
    owner: &str,
    reason_code: &'static str,
) -> Result<(), &'static str> {
    jobs::reconcile_communication_action(
        pool,
        &JobsCommunicationActionReconciliation {
            lease: lease_access(lease, owner),
            resolution: "inconclusive".to_string(),
            provider_object_id: String::new(),
            evidence: bound_reconciliation_evidence(
                lease,
                json!({
                    "schema_version": 1,
                    "reason_code": reason_code,
                }),
            ),
        },
    )
    .map(|_| ())
    .map_err(|_| "reconciliation_evidence_failed")
}

fn absence_age_ready(lease: &JobsCommunicationActionLease) -> bool {
    lease
        .action
        .dispatched_at_ms
        .is_some_and(|dispatched_at_ms| {
            jobs::now_ms().saturating_sub(dispatched_at_ms) >= AUTHORITATIVE_ABSENCE_AGE_MS
        })
}

#[derive(Clone, Copy)]
enum CredentialUse {
    Dispatch,
    Lookup,
}

#[derive(Clone, Copy)]
struct PreparationFailure {
    reason_code: &'static str,
    authorization: bool,
}

impl PreparationFailure {
    fn unavailable(reason_code: &'static str) -> Self {
        Self {
            reason_code,
            authorization: false,
        }
    }

    fn authorization(reason_code: &'static str) -> Self {
        Self {
            reason_code,
            authorization: true,
        }
    }
}

async fn prepare_credential(
    pool: &DbPool,
    client: &reqwest::Client,
    lease: &JobsCommunicationActionLease,
    credential_use: CredentialUse,
) -> Result<JobsProviderCredential, PreparationFailure> {
    let mailbox = jobs::mailbox_connection(pool, &lease.account_id, &lease.action.connection_id)
        .map_err(|_| PreparationFailure::unavailable("mailbox_lookup_failed"))?
        .ok_or_else(|| PreparationFailure::unavailable("mailbox_connection_missing"))?;
    let mut credential =
        jobs::jobs_provider_credential(pool, &lease.account_id, &lease.action.connection_id)
            .map_err(|_| PreparationFailure::unavailable("provider_credential_lookup_failed"))?
            .ok_or_else(|| PreparationFailure::authorization("provider_credential_missing"))?;
    validate_credential(lease, &mailbox, &credential, credential_use)?;
    if credential.expires_at_ms <= jobs::now_ms().saturating_add(REFRESH_SKEW_MS) {
        credential = refresh_credential(pool, client, &lease.account_id, credential).await?;
        validate_credential(lease, &mailbox, &credential, credential_use)?;
    }
    if credential.expires_at_ms <= jobs::now_ms() {
        return Err(PreparationFailure::authorization(
            "provider_access_token_expired",
        ));
    }
    Ok(credential)
}

fn validate_credential(
    lease: &JobsCommunicationActionLease,
    mailbox: &MailboxConnection,
    credential: &JobsProviderCredential,
    credential_use: CredentialUse,
) -> Result<(), PreparationFailure> {
    let Some(connection_provider) = connection_provider(&lease.action.provider) else {
        return Err(PreparationFailure::unavailable(
            "communication_provider_invalid",
        ));
    };
    if mailbox.status != "connected"
        || mailbox.id != lease.action.connection_id
        || mailbox.provider != connection_provider
        || credential.connection_id != lease.action.connection_id
        || credential.provider != connection_provider
        || credential.provider_subject.trim().is_empty()
    {
        return Err(PreparationFailure::authorization(
            "provider_connection_not_ready",
        ));
    }
    if credential.grant_revision > 0 {
        let digest = jobs::communication_grant_sha256(credential)
            .map_err(|_| PreparationFailure::authorization("provider_grant_digest_invalid"))?;
        if credential.grant_sha256 != digest {
            return Err(PreparationFailure::authorization(
                "provider_grant_digest_invalid",
            ));
        }
    }
    match credential_use {
        CredentialUse::Dispatch => {
            if credential.grant_revision != lease.grant_revision
                || credential.grant_sha256 != lease.grant_sha256
                || !action_provider_is_granted(
                    &credential.provider,
                    &lease.action.provider,
                    &credential.scopes,
                    &credential.capabilities,
                )
            {
                return Err(PreparationFailure::authorization(
                    "provider_write_grant_changed",
                ));
            }
        }
        CredentialUse::Lookup => {
            if !action_provider_lookup_is_granted(
                &credential.provider,
                &lease.action.provider,
                &credential.scopes,
            ) {
                return Err(PreparationFailure::authorization(
                    "provider_lookup_grant_missing",
                ));
            }
        }
    }
    Ok(())
}

async fn refresh_credential(
    pool: &DbPool,
    client: &reqwest::Client,
    account_id: &str,
    credential: JobsProviderCredential,
) -> Result<JobsProviderCredential, PreparationFailure> {
    let purpose = if credential.grant_revision > 0
        && credential.capabilities.iter().any(|capability| {
            matches!(
                capability.as_str(),
                "recruiter_reply" | "interview_calendar"
            )
        }) {
        ProviderAuthorizationPurpose::CommunicationWrite
    } else {
        ProviderAuthorizationPurpose::MailboxRead
    };
    let config = provider_config(&credential.provider, purpose)
        .ok_or_else(|| PreparationFailure::unavailable("provider_refresh_not_configured"))?;
    let token = refresh_access_token(client, &config, &credential.refresh_token)
        .await
        .map_err(|error| match error.kind() {
            ProviderAuthErrorKind::ReauthorizationRequired => {
                PreparationFailure::authorization("provider_refresh_reauthorization_required")
            }
            ProviderAuthErrorKind::Transient => {
                PreparationFailure::unavailable("provider_refresh_transient")
            }
        })?;
    if !token.scopes.is_empty()
        && normalized_scopes(token.scopes.iter().map(String::as_str))
            != normalized_scopes(credential.scopes.iter().map(String::as_str))
    {
        return Err(PreparationFailure::authorization(
            "provider_refresh_scope_changed",
        ));
    }
    let expires_at_ms =
        jobs::now_ms().saturating_add(token.expires_in_seconds.max(60).saturating_mul(1_000));
    let rotated_refresh_token =
        (!token.refresh_token.trim().is_empty()).then_some(token.refresh_token.as_str());
    jobs::refresh_jobs_provider_credential_cas(
        pool,
        account_id,
        &credential,
        &token.access_token,
        rotated_refresh_token,
        expires_at_ms,
    )
    .map_err(|_| PreparationFailure::unavailable("provider_refresh_cas_lost"))
}

fn provider_request(
    pool: &DbPool,
    lease: &JobsCommunicationActionLease,
) -> Result<CommunicationProviderRequest, &'static str> {
    let source_message = if lease.action.kind == "reply" {
        let source_id = lease
            .action
            .source_message_id
            .as_deref()
            .ok_or("reply_source_message_missing")?;
        let message = jobs::provider_message(
            pool,
            &lease.account_id,
            &lease.action.connection_id,
            source_id,
        )
        .map_err(|_| "reply_source_lookup_failed")?
        .ok_or("reply_source_message_missing")?;
        if message.connection_id != lease.action.connection_id
            || message.application_id.as_deref() != Some(lease.action.application_id.as_str())
            || connection_provider(&lease.action.provider) != Some(message.provider.as_str())
            || message.external_id.trim().is_empty()
        {
            return Err("reply_source_binding_changed");
        }
        let provider_id = metadata_text(&message.metadata, "provider_id")
            .unwrap_or_else(|| message.external_id.clone());
        let rfc_message_id = metadata_text(&message.metadata, "rfc_message_id")
            .or_else(|| {
                message
                    .external_id
                    .starts_with('<')
                    .then(|| message.external_id.clone())
            })
            .unwrap_or_default();
        Some(CommunicationSourceMessage {
            provider: message.provider,
            provider_id,
            external_id: message.external_id,
            rfc_message_id,
            thread_id: metadata_text(&message.metadata, "thread_id").unwrap_or_default(),
            conversation_id: metadata_text(&message.metadata, "conversation_id")
                .unwrap_or_default(),
            sender: message.sender,
            reply_target: metadata_text(&message.metadata, "reply_target").unwrap_or_default(),
            subject: message.subject,
        })
    } else {
        None
    };
    Ok(CommunicationProviderRequest {
        action_id: lease.action.id.clone(),
        provider: lease.action.provider.clone(),
        provider_operation_key: lease.provider_operation_key.clone(),
        payload_sha256: lease.action.payload_sha256.clone(),
        payload: lease.action.payload.clone(),
        source_message,
    })
}

fn metadata_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 2_048)
        .map(str::to_string)
}

fn connection_provider(action_provider: &str) -> Option<&'static str> {
    match action_provider {
        "gmail" | "google_calendar" => Some("gmail"),
        "outlook_email" | "outlook_calendar" => Some("outlook"),
        _ => None,
    }
}

fn lease_access(lease: &JobsCommunicationActionLease, owner: &str) -> JobsCommunicationLeaseAccess {
    JobsCommunicationLeaseAccess {
        account_id: lease.account_id.clone(),
        action_id: lease.action.id.clone(),
        attempt_id: lease.attempt_id.clone(),
        owner_id: owner.to_string(),
        lease_token: lease.lease_token.clone(),
        fence: lease.fence,
        authority_sha256: lease.authority_sha256.clone(),
        approval_revision: lease.approval_revision,
        grant_revision: lease.grant_revision,
        grant_sha256: lease.grant_sha256.clone(),
    }
}

fn bound_evidence(request: &CommunicationProviderRequest, evidence: Value) -> Value {
    let mut object = evidence.as_object().cloned().unwrap_or_default();
    object.insert(
        "provider".to_string(),
        Value::String(request.provider.clone()),
    );
    object.insert(
        "action_id_sha256".to_string(),
        Value::String(hex::encode(Sha256::digest(request.action_id.as_bytes()))),
    );
    object.insert(
        "payload_sha256".to_string(),
        Value::String(request.payload_sha256.clone()),
    );
    object.insert(
        "provider_operation_key_sha256".to_string(),
        Value::String(hex::encode(Sha256::digest(
            request.provider_operation_key.as_bytes(),
        ))),
    );
    Value::Object(object)
}

fn bound_reconciliation_evidence(lease: &JobsCommunicationActionLease, evidence: Value) -> Value {
    let mut object = evidence.as_object().cloned().unwrap_or_default();
    object.insert(
        "provider".to_string(),
        Value::String(lease.action.provider.clone()),
    );
    object.insert(
        "action_id_sha256".to_string(),
        Value::String(hex::encode(Sha256::digest(lease.action.id.as_bytes()))),
    );
    object.insert(
        "payload_sha256".to_string(),
        Value::String(lease.action.payload_sha256.clone()),
    );
    object.insert(
        "provider_operation_key_sha256".to_string(),
        Value::String(hex::encode(Sha256::digest(
            lease.provider_operation_key.as_bytes(),
        ))),
    );
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    struct PendingProviderDispatch {
        polls: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }

    impl std::future::Future for PendingProviderDispatch {
        type Output = ProviderDispatchResult;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::Pending
        }
    }

    impl Drop for PendingProviderDispatch {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn provider_client_never_follows_redirects() {
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
        let response = provider_client()
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
    async fn expired_dispatch_lease_never_constructs_or_polls_provider_request() {
        let dispatches = Arc::new(AtomicUsize::new(0));
        let attempted = Arc::clone(&dispatches);
        let result = dispatch_before_lease_deadline(Some(4_999), 5_000, move || {
            attempted.fetch_add(1, Ordering::SeqCst);
            async {
                ProviderDispatchResult::Ambiguous {
                    reason_code: "must_not_dispatch",
                }
            }
        })
        .await;

        assert_eq!(result.unwrap_err(), "dispatch_lease_deadline_expired");
        assert_eq!(dispatches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn in_flight_provider_dispatch_is_dropped_at_absolute_lease_deadline() {
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let dispatch_polls = Arc::clone(&polls);
        let dispatch_drops = Arc::clone(&drops);
        let task = tokio::spawn(async move {
            dispatch_before_lease_deadline(Some(10_100), 10_000, move || PendingProviderDispatch {
                polls: dispatch_polls,
                drops: dispatch_drops,
            })
            .await
        });

        tokio::task::yield_now().await;
        assert!(polls.load(Ordering::SeqCst) > 0);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        tokio::time::advance(Duration::from_millis(100)).await;
        let result = task.await.unwrap();
        assert_eq!(result.unwrap_err(), "dispatch_lease_deadline_elapsed");
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        let polls_at_deadline = polls.load(Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(60)).await;
        tokio::task::yield_now().await;
        assert_eq!(polls.load(Ordering::SeqCst), polls_at_deadline);
    }
}
