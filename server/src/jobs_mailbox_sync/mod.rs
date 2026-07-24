//! Durable read-only mailbox synchronization for Bluey Jobs.
//!
//! Provider messages are persisted idempotently, correlated to one exact
//! application, and converted into evidence or review interventions. This
//! worker never sends mail and never changes an employer outcome by inference.

mod processing;
mod providers;

use crate::{
    api::jobs_mailbox_oauth::provider_config,
    db::{
        jobs::{self, JobsProviderCredential, JobsProviderSyncState},
        DbPool,
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
            .user_agent("bluey-jobs-mailbox-sync/1")
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(error = %error, "failed to create Jobs mailbox sync client");
                return;
            }
        };
        let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = run_sync_cycle(&pool, &client, &owner).await {
                tracing::warn!(error = %error, "Jobs mailbox sync cycle failed");
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
                error = %error,
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
    let config = provider_config(&state.provider)
        .ok_or_else(|| anyhow::anyhow!("mailbox provider is not configured"))?;
    let mut credential = jobs::jobs_provider_credential(pool, account_id, &state.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox credential not found"))?;
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
    config: &crate::api::jobs_mailbox_oauth::ProviderConfig,
    credential: &mut JobsProviderCredential,
) -> anyhow::Result<()> {
    if credential.expires_at_ms > jobs::now_ms().saturating_add(60_000) {
        return Ok(());
    }
    let token = providers::refresh_access_token(client, config, &credential.refresh_token).await?;
    credential.access_token = token.access_token;
    if !token.refresh_token.trim().is_empty() {
        credential.refresh_token = token.refresh_token;
    }
    if !token.scopes.is_empty() {
        credential.scopes = token.scopes;
    }
    credential.expires_at_ms =
        jobs::now_ms().saturating_add(token.expires_in_seconds.max(60) * 1_000);
    credential.updated_at_ms = jobs::now_ms();
    jobs::save_jobs_provider_credential(pool, account_id, credential)?;
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

fn authorization_needs_attention(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("401")
        || text.contains("403")
        || text.contains("refresh token")
        || text.contains("invalid_grant")
}

#[cfg(test)]
mod tests {
    use super::{authorization_needs_attention, env_value_enabled};

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
}
