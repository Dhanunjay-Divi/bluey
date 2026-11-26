//! Durable read-only mailbox synchronization for Bluey Jobs.
//!
//! Provider messages are persisted idempotently, correlated to one exact
//! application, and converted into evidence or review interventions. This
//! worker never sends mail and never changes an employer outcome by inference.

mod processing;
mod providers;

use crate::{
    db::{
        jobs::{self, JobsProviderCredential, JobsProviderSyncState},
        DbPool,
    },
    jobs_provider_auth::{
        normalized_scopes, provider_config, refresh_access_token, ProviderAuthErrorKind,
        ProviderAuthorizationPurpose, ProviderConfig, RefreshResult,
    },
};
use std::time::Duration;
use tokio::task::JoinHandle;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;
const LEASE_MS: i64 = 2 * 60 * 1_000;
const ERROR_BACKOFF_MS: i64 = 5 * 60 * 1_000;
const CLAIM_BATCH_SIZE: usize = 25;
const MESSAGE_BATCH_SIZE: usize = 100;

pub fn spawn_mailbox_sync_worker(pool: DbPool) -> Option<JoinHandle<()>> {
    if !env_enabled("BLUEY_JOBS_MAILBOX_SYNC_ENABLED", false) {
        tracing::info!("Jobs mailbox sync worker disabled");
        return None;
    }
    let poll_interval = std::env::var("BLUEY_JOBS_MAILBOX_SYNC_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
        .clamp(5, 300);
    let owner = format!("mailbox-sync-{}", uuid::Uuid::new_v4());
    Some(tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("bluey-jobs-mailbox-sync/1")
            .build()
        {
            Ok(client) => client,
            Err(_) => {
                tracing::error!(
                    reason_code = "client_initialization_failed",
                    "failed to create Jobs mailbox sync client"
                );
                return;
            }
        };
        let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = run_sync_cycle(&pool, &client, &owner).await {
                tracing::warn!(
                    reason_code = sync_failure_reason_code(&error),
                    "Jobs mailbox sync cycle failed"
                );
            }
        }
    }))
}

async fn run_sync_cycle(
    pool: &DbPool,
    client: &reqwest::Client,
    owner: &str,
) -> anyhow::Result<()> {
    let claimed = jobs::claim_due_mailbox_syncs(pool, owner, LEASE_MS, CLAIM_BATCH_SIZE)?;
    for (account_id, state) in claimed {
        if let Err(error) = sync_connection(pool, client, owner, &account_id, &state).await {
            let next_sync = jobs::now_ms().saturating_add(ERROR_BACKOFF_MS);
            if authorization_needs_attention(&error) {
                let _ = jobs::mark_mailbox_reauthorization_required(
                    pool,
                    &account_id,
                    &state.connection_id,
                );
            }
            let _ = jobs::finish_mailbox_sync(
                pool,
                &account_id,
                &state.connection_id,
                owner,
                state.cursor.clone(),
                &public_sync_error(&error),
                Some(next_sync),
            );
            tracing::warn!(
                provider = %state.provider,
                connection_id = %state.connection_id,
                reason_code = sync_failure_reason_code(&error),
                "Jobs mailbox connection sync failed"
            );
        }
    }
    Ok(())
}

async fn sync_connection(
    pool: &DbPool,
    client: &reqwest::Client,
    owner: &str,
    account_id: &str,
    state: &JobsProviderSyncState,
) -> anyhow::Result<()> {
    let mailbox = jobs::mailbox_connection(pool, account_id, &state.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox connection not found"))?;
    if mailbox.status != "connected" || mailbox.provider != state.provider {
        anyhow::bail!("mailbox connection is not ready")
    }
    let mut credential = jobs::jobs_provider_credential(pool, account_id, &state.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox credential not found"))?;
    let config = provider_config(&state.provider, credential_refresh_purpose(&credential))
        .ok_or_else(|| anyhow::anyhow!("mailbox provider is not configured"))?;
    refresh_credential_if_needed(pool, client, account_id, &config, &mut credential).await?;

    let fetched = providers::fetch_messages(
        client,
        &config,
        &credential,
        state.last_synced_at_ms,
        &state.cursor,
    )
    .await?;
    for message in fetched.messages {
        let provider_message = message.into_stored(&state.connection_id, &state.provider);
        let _ = jobs::save_provider_message(pool, account_id, &provider_message)?;
    }

    let pending = jobs::list_pending_provider_messages(
        pool,
        account_id,
        &state.connection_id,
        MESSAGE_BATCH_SIZE,
    )?;
    for message in pending {
        processing::process_provider_message(pool, account_id, &message)?;
    }

    if !jobs::finish_mailbox_sync(
        pool,
        account_id,
        &state.connection_id,
        owner,
        fetched.cursor,
        "",
        None,
    )? {
        anyhow::bail!("mailbox sync lease was lost before completion")
    }
    Ok(())
}

async fn refresh_credential_if_needed(
    pool: &DbPool,
    client: &reqwest::Client,
    account_id: &str,
    config: &ProviderConfig,
    credential: &mut JobsProviderCredential,
) -> anyhow::Result<()> {
    if credential.expires_at_ms > jobs::now_ms().saturating_add(60_000) {
        return Ok(());
    }
    let token = refresh_access_token(client, config, &credential.refresh_token)
        .await
        .map_err(|error| match error.kind() {
            ProviderAuthErrorKind::ReauthorizationRequired => {
                anyhow::anyhow!("mailbox authorization needs attention")
            }
            ProviderAuthErrorKind::Transient => {
                anyhow::anyhow!("provider token refresh did not complete")
            }
        })?;
    let expected = credential.clone();
    let access_token = token.access_token.clone();
    let rotated_refresh_token =
        (!token.refresh_token.trim().is_empty()).then(|| token.refresh_token.clone());
    apply_refresh_result(credential, token)?;
    *credential = jobs::refresh_jobs_provider_credential_cas(
        pool,
        account_id,
        &expected,
        &access_token,
        rotated_refresh_token.as_deref(),
        credential.expires_at_ms,
    )?;
    Ok(())
}

fn credential_refresh_purpose(credential: &JobsProviderCredential) -> ProviderAuthorizationPurpose {
    if credential.grant_revision > 0
        && credential.capabilities.iter().any(|capability| {
            matches!(
                capability.as_str(),
                "recruiter_reply" | "interview_calendar"
            )
        })
    {
        ProviderAuthorizationPurpose::CommunicationWrite
    } else {
        ProviderAuthorizationPurpose::MailboxRead
    }
}

fn apply_refresh_result(
    credential: &mut JobsProviderCredential,
    token: RefreshResult,
) -> anyhow::Result<()> {
    if credential.grant_revision > 0 {
        let expected = jobs::communication_grant_sha256(credential)?;
        if credential.grant_sha256 != expected {
            anyhow::bail!("provider grant digest is invalid before token refresh")
        }
    }
    if !token.scopes.is_empty() {
        let returned = normalized_scopes(token.scopes.iter().map(String::as_str));
        let existing = normalized_scopes(credential.scopes.iter().map(String::as_str));
        if returned != existing {
            anyhow::bail!("provider permissions changed during token refresh")
        }
    }
    credential.access_token = token.access_token;
    if !token.refresh_token.trim().is_empty() {
        credential.refresh_token = token.refresh_token;
    }
    credential.expires_at_ms =
        jobs::now_ms().saturating_add(token.expires_in_seconds.max(60) * 1_000);
    credential.updated_at_ms = jobs::now_ms();
    if credential.grant_revision > 0 {
        credential.grant_sha256 = jobs::communication_grant_sha256(credential)?;
    }
    Ok(())
}

fn env_enabled(name: &str, default: bool) -> bool {
    env_value_enabled(std::env::var(name).ok().as_deref(), default)
}

fn env_value_enabled(value: Option<&str>, default: bool) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn public_sync_error(error: &anyhow::Error) -> String {
    if authorization_needs_attention(error) {
        "Mailbox authorization needs attention.".to_string()
    } else if error.to_string().contains("configured") {
        "Mailbox provider is temporarily unavailable.".to_string()
    } else {
        "Mailbox sync could not complete. Bluey will retry.".to_string()
    }
}

fn sync_failure_reason_code(error: &anyhow::Error) -> &'static str {
    if authorization_needs_attention(error) {
        "authorization_required"
    } else if error.to_string().contains("configured") {
        "provider_unavailable"
    } else if error.to_string().contains("response was invalid") {
        "provider_response_invalid"
    } else {
        "retryable_sync_failure"
    }
}

fn authorization_needs_attention(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("401")
        || text.contains("403")
        || text.contains("refresh token")
        || text.contains("invalid_grant")
}

#[cfg(test)]
mod tests {
    use super::{
        apply_refresh_result, authorization_needs_attention, credential_refresh_purpose,
        env_value_enabled, public_sync_error, sync_failure_reason_code,
    };
    use crate::{
        db::jobs::{self, JobsProviderCredential},
        jobs_provider_auth::{
            ProviderAuthorizationPurpose, RefreshResult, GOOGLE_CALENDAR_EVENTS_SCOPE,
            GOOGLE_GMAIL_READ_SCOPE, GOOGLE_GMAIL_SEND_SCOPE,
        },
    };

    #[test]
    fn mailbox_sync_requires_explicit_enablement() {
        assert!(!env_value_enabled(None, false));
        assert!(!env_value_enabled(Some("0"), false));
        assert!(!env_value_enabled(Some("false"), false));
        assert!(env_value_enabled(Some("1"), false));
        assert!(env_value_enabled(Some(" yes "), false));
    }

    #[test]
    fn authorization_failures_require_reconnection() {
        for message in [
            "provider returned 401 Unauthorized",
            "provider returned 403 Forbidden",
            "refresh token is missing",
            "oauth error: invalid_grant",
        ] {
            assert!(
                authorization_needs_attention(&anyhow::anyhow!(message)),
                "{message}"
            );
        }
    }

    #[test]
    fn transient_provider_failures_remain_retryable() {
        for message in [
            "request timed out",
            "provider returned 500",
            "mailbox provider is temporarily unavailable",
        ] {
            assert!(
                !authorization_needs_attention(&anyhow::anyhow!(message)),
                "{message}"
            );
        }
    }

    #[test]
    fn mailbox_sync_errors_are_publicly_coarsened_without_provider_secrets() {
        let secret = "https://graph.microsoft.com/delta?$skiptoken=PRIVATE-SENTINEL";
        let error = anyhow::anyhow!("provider failed at {secret}");
        let public = public_sync_error(&error);
        let reason = sync_failure_reason_code(&error);
        assert!(!public.contains("PRIVATE-SENTINEL"));
        assert!(!reason.contains("PRIVATE-SENTINEL"));
        assert_eq!(reason, "retryable_sync_failure");
    }

    #[test]
    fn write_grant_survives_token_rotation_without_scope_drift() {
        let mut credential = JobsProviderCredential {
            connection_id: "connection-1".to_string(),
            provider: "gmail".to_string(),
            provider_subject: "subject-1".to_string(),
            access_token: "dummy-old-access-token".to_string(),
            refresh_token: "dummy-old-refresh-token".to_string(),
            scopes: vec![
                GOOGLE_GMAIL_READ_SCOPE.to_string(),
                GOOGLE_GMAIL_SEND_SCOPE.to_string(),
                GOOGLE_CALENDAR_EVENTS_SCOPE.to_string(),
            ],
            capabilities: vec![
                "status_sync".to_string(),
                "application_correlation".to_string(),
                "review_interventions".to_string(),
                "recruiter_reply".to_string(),
                "interview_calendar".to_string(),
            ],
            grant_revision: 3,
            grant_sha256: String::new(),
            expires_at_ms: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        credential.grant_sha256 = jobs::communication_grant_sha256(&credential).unwrap();
        let original_scopes = credential.scopes.clone();
        let original_capabilities = credential.capabilities.clone();
        let original_revision = credential.grant_revision;
        let original_digest = credential.grant_sha256.clone();

        assert_eq!(
            credential_refresh_purpose(&credential),
            ProviderAuthorizationPurpose::CommunicationWrite
        );
        apply_refresh_result(
            &mut credential,
            RefreshResult {
                access_token: "dummy-rotated-access-token".to_string(),
                refresh_token: "dummy-rotated-refresh-token".to_string(),
                scopes: Vec::new(),
                expires_in_seconds: 3_600,
            },
        )
        .unwrap();

        assert_eq!(credential.scopes, original_scopes);
        assert_eq!(credential.capabilities, original_capabilities);
        assert_eq!(credential.grant_revision, original_revision);
        assert_eq!(credential.grant_sha256, original_digest);
        assert_eq!(
            credential.grant_sha256,
            jobs::communication_grant_sha256(&credential).unwrap()
        );
        assert_eq!(credential.access_token, "dummy-rotated-access-token");
        assert_eq!(credential.refresh_token, "dummy-rotated-refresh-token");
    }

    #[test]
    fn refresh_rejects_silent_write_scope_downgrade() {
        let mut credential = JobsProviderCredential {
            connection_id: "connection-2".to_string(),
            provider: "gmail".to_string(),
            provider_subject: "subject-2".to_string(),
            access_token: "dummy-old-access-token".to_string(),
            refresh_token: "dummy-old-refresh-token".to_string(),
            scopes: vec![
                GOOGLE_GMAIL_READ_SCOPE.to_string(),
                GOOGLE_GMAIL_SEND_SCOPE.to_string(),
            ],
            capabilities: vec!["status_sync".to_string(), "recruiter_reply".to_string()],
            grant_revision: 2,
            grant_sha256: String::new(),
            expires_at_ms: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        credential.grant_sha256 = jobs::communication_grant_sha256(&credential).unwrap();
        let error = apply_refresh_result(
            &mut credential,
            RefreshResult {
                access_token: "dummy-new-access-token".to_string(),
                refresh_token: String::new(),
                scopes: vec![GOOGLE_GMAIL_READ_SCOPE.to_string()],
                expires_in_seconds: 3_600,
            },
        )
        .expect_err("a missing Gmail send scope must fail closed");
        assert!(error
            .to_string()
            .contains("permissions changed during token refresh"));
        assert_eq!(credential.access_token, "dummy-old-access-token");
    }
}
