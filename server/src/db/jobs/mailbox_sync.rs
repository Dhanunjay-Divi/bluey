const MAILBOX_SYNC_DEFAULT_INTERVAL_MS: i64 = 2 * 60 * 1_000;
const MAILBOX_SYNC_MAX_BATCH: usize = 50;
const MAILBOX_MESSAGE_LIST_MAX: usize = 200;

type MailboxSyncStateParts = (
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
);

type ProviderMessageParts = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    Option<i64>,
    i64,
    i64,
);

fn normalize_mailbox_provider(provider: &str) -> Result<String> {
    let provider = provider.trim().to_ascii_lowercase();
    if !matches!(provider.as_str(), "gmail" | "outlook") {
        anyhow::bail!("unsupported mailbox provider")
    }
    Ok(provider)
}

fn mailbox_sync_state_from_parts(parts: MailboxSyncStateParts) -> Result<JobsProviderSyncState> {
    let (
        connection_id,
        provider,
        sync_json,
        next_sync_at_ms,
        last_synced_at_ms,
        lease_owner,
        lease_expires_at_ms,
        created_at_ms,
        updated_at_ms,
    ) = parts;
    let mut state: JobsProviderSyncState = parse_json(sync_json, "Jobs provider sync state")?;
    state.connection_id = connection_id;
    state.provider = provider;
    state.next_sync_at_ms = next_sync_at_ms;
    state.last_synced_at_ms = last_synced_at_ms;
    state.lease_owner = lease_owner;
    state.lease_expires_at_ms = lease_expires_at_ms;
    state.created_at_ms = created_at_ms;
    state.updated_at_ms = updated_at_ms;
    Ok(state)
}

fn provider_message_from_parts(parts: ProviderMessageParts) -> Result<JobsProviderMessage> {
    let (
        id,
        connection_id,
        provider,
        application_id,
        processing_status,
        message_json,
        received_at_ms,
        processed_at_ms,
        created_at_ms,
        updated_at_ms,
    ) = parts;
    let mut message: JobsProviderMessage = parse_json(message_json, "Jobs provider message")?;
    message.id = id;
    message.connection_id = connection_id;
    message.provider = provider;
    message.application_id = application_id;
    message.processing_status = processing_status;
    message.received_at_ms = received_at_ms;
    message.processed_at_ms = processed_at_ms;
    message.created_at_ms = created_at_ms;
    message.updated_at_ms = updated_at_ms;
    Ok(message)
}

pub fn mailbox_connection(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<Option<MailboxConnection>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<String> = pool
                .get()?
                .query_row(
                    "SELECT connection_json FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, connection_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "mailbox connection"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT connection_json FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &connection_id],
            )?
            .map(|row| parse_json(row.get(0), "mailbox connection"))
            .transpose(),
    })
}

pub fn initialize_mailbox_sync_state(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
    provider: &str,
) -> Result<JobsProviderSyncState> {
    let provider = normalize_mailbox_provider(provider)?;
    let mailbox = mailbox_connection(pool, account_id, connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox connection not found"))?;
    if mailbox.provider != provider || mailbox.status != "connected" {
        anyhow::bail!("mailbox connection is not ready for sync")
    }
    let now = now_ms();
    let state = JobsProviderSyncState {
        connection_id: connection_id.to_string(),
        provider: provider.clone(),
        cursor: json!({}),
        next_sync_at_ms: now,
        last_synced_at_ms: None,
        last_error: String::new(),
        lease_owner: None,
        lease_expires_at_ms: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    let payload = to_json(&state, "Jobs provider sync state")?;
    crate::db::run_blocking_db(|| -> Result<()> {
        match pool {
            DbPool::Sqlite(_) => {
                pool.get()?.execute(
                    "INSERT INTO jobs_provider_sync_state (
                    connection_id, account_id, provider, sync_json, next_sync_at_ms,
                    last_synced_at_ms, lease_owner, lease_expires_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, ?6, ?7)
                 ON CONFLICT(connection_id) DO NOTHING",
                    params![connection_id, account_id, provider, payload, now, now, now,],
                )?;
                Ok(())
            }
            DbPool::Postgres(_) => {
                pool.get_pg()?.execute(
                    "INSERT INTO jobs_provider_sync_state (
                    connection_id, account_id, provider, sync_json, next_sync_at_ms,
                    last_synced_at_ms, lease_owner, lease_expires_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, NULL, NULL, NULL, $6, $7)
                 ON CONFLICT(connection_id) DO NOTHING",
                    &[
                        &connection_id,
                        &account_id,
                        &provider,
                        &payload,
                        &now,
                        &now,
                        &now,
                    ],
                )?;
                Ok(())
            }
        }
    })?;
    mailbox_sync_state(pool, account_id, connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox sync state was not created"))
}

pub fn mailbox_sync_state(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<Option<JobsProviderSyncState>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let row: Option<MailboxSyncStateParts> = pool
                .get()?
                .query_row(
                    "SELECT s.connection_id, s.provider, s.sync_json, s.next_sync_at_ms,
                            s.last_synced_at_ms, s.lease_owner, s.lease_expires_at_ms,
                            s.created_at_ms, s.updated_at_ms
                       FROM jobs_provider_sync_state AS s
                       JOIN jobs_mailbox_connections AS c
                         ON c.id = s.connection_id AND c.account_id = s.account_id
                      WHERE s.account_id = ?1 AND s.connection_id = ?2
                        AND c.status = 'connected'",
                    params![account_id, connection_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    },
                )
                .optional()?;
            row.map(|value| {
                mailbox_sync_state_from_parts((
                    value.0, value.1, value.2, value.3, value.4, value.5, value.6, value.7, value.8,
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT connection_id, provider, sync_json, next_sync_at_ms,
                        last_synced_at_ms, lease_owner, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                   FROM jobs_provider_sync_state
                  WHERE account_id = $1 AND connection_id = $2",
                &[&account_id, &connection_id],
            )?
            .map(|row| {
                mailbox_sync_state_from_parts((
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    row.get(7),
                    row.get(8),
                ))
            })
            .transpose(),
    })
}

pub fn claim_mailbox_sync(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
    owner: &str,
    lease_ms: i64,
) -> Result<Option<JobsProviderSyncState>> {
    let owner = owner.trim();
    if owner.is_empty() {
        anyhow::bail!("mailbox sync owner is required")
    }
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(lease_ms.max(1_000));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row: Option<MailboxSyncStateParts> = tx
                .query_row(
                    "SELECT connection_id, provider, sync_json, next_sync_at_ms,
                            last_synced_at_ms, lease_owner, lease_expires_at_ms,
                            created_at_ms, updated_at_ms
                       FROM jobs_provider_sync_state
                      WHERE account_id = ?1 AND connection_id = ?2",
                    params![account_id, connection_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    },
                )
                .optional()?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            if row.6.is_some_and(|expires| expires > now) && row.5.as_deref() != Some(owner) {
                tx.commit()?;
                return Ok(None);
            }
            let changed = tx.execute(
                "UPDATE jobs_provider_sync_state
                    SET lease_owner = ?3, lease_expires_at_ms = ?4, updated_at_ms = ?5
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?5 OR lease_owner = ?3)
                    AND EXISTS (
                        SELECT 1
                          FROM jobs_mailbox_connections AS c
                         WHERE c.id = jobs_provider_sync_state.connection_id
                           AND c.account_id = jobs_provider_sync_state.account_id
                           AND c.status = 'connected'
                    )",
                params![
                    account_id,
                    connection_id,
                    owner,
                    lease_expires_at_ms,
                    now,
                ],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }
            tx.commit()?;
            let mut state = mailbox_sync_state_from_parts((
                row.0,
                row.1,
                row.2,
                row.3,
                row.4,
                Some(owner.to_string()),
                Some(lease_expires_at_ms),
                row.7,
                now,
            ))?;
            state.lease_owner = Some(owner.to_string());
            state.lease_expires_at_ms = Some(lease_expires_at_ms);
            Ok(Some(state))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT s.connection_id, s.provider, s.sync_json, s.next_sync_at_ms,
                        s.last_synced_at_ms, s.lease_owner, s.lease_expires_at_ms,
                        s.created_at_ms, s.updated_at_ms
                   FROM jobs_provider_sync_state AS s
                   JOIN jobs_mailbox_connections AS c
                     ON c.id = s.connection_id AND c.account_id = s.account_id
                  WHERE s.account_id = $1 AND s.connection_id = $2
                    AND c.status = 'connected'
                  FOR UPDATE OF s",
                &[&account_id, &connection_id],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let current_owner: Option<String> = row.get(5);
            let current_expiry: Option<i64> = row.get(6);
            if current_expiry.is_some_and(|expires| expires > now)
                && current_owner.as_deref() != Some(owner)
            {
                tx.commit()?;
                return Ok(None);
            }
            tx.execute(
                "UPDATE jobs_provider_sync_state
                    SET lease_owner = $3, lease_expires_at_ms = $4, updated_at_ms = $5
                  WHERE account_id = $1 AND connection_id = $2
                    AND EXISTS (
                        SELECT 1
                          FROM jobs_mailbox_connections AS c
                         WHERE c.id = jobs_provider_sync_state.connection_id
                           AND c.account_id = jobs_provider_sync_state.account_id
                           AND c.status = 'connected'
                    )",
                &[
                    &account_id,
                    &connection_id,
                    &owner,
                    &lease_expires_at_ms,
                    &now,
                ],
            )?;
            let state = mailbox_sync_state_from_parts((
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                Some(owner.to_string()),
                Some(lease_expires_at_ms),
                row.get(7),
                now,
            ))?;
            tx.commit()?;
            Ok(Some(state))
        }
    })
}

pub fn claim_due_mailbox_syncs(
    pool: &DbPool,
    owner: &str,
    lease_ms: i64,
    limit: usize,
) -> Result<Vec<(String, JobsProviderSyncState)>> {
    let owner = owner.trim();
    if owner.is_empty() {
        anyhow::bail!("mailbox sync owner is required")
    }
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(lease_ms.max(1_000));
    let limit = limit.clamp(1, MAILBOX_SYNC_MAX_BATCH) as i64;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let rows = {
                let mut stmt = tx.prepare(
                    "SELECT s.account_id, s.connection_id, s.provider, s.sync_json,
                            s.next_sync_at_ms, s.last_synced_at_ms,
                            s.created_at_ms, s.updated_at_ms
                       FROM jobs_provider_sync_state AS s
                       JOIN jobs_mailbox_connections AS c
                         ON c.id = s.connection_id AND c.account_id = s.account_id
                      WHERE s.next_sync_at_ms <= ?1
                        AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= ?1)
                        AND c.status = 'connected'
                      ORDER BY s.next_sync_at_ms ASC, s.connection_id ASC
                      LIMIT ?2",
                )?;
                let mapped = stmt.query_map(params![now, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                })?;
                mapped.collect::<std::result::Result<Vec<_>, _>>()?
            };
            let mut claimed = Vec::with_capacity(rows.len());
            for row in rows {
                let changed = tx.execute(
                    "UPDATE jobs_provider_sync_state
                        SET lease_owner = ?3, lease_expires_at_ms = ?4, updated_at_ms = ?5
                      WHERE account_id = ?1 AND connection_id = ?2
                        AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?5)
                        AND EXISTS (
                            SELECT 1
                              FROM jobs_mailbox_connections AS c
                             WHERE c.id = jobs_provider_sync_state.connection_id
                               AND c.account_id = jobs_provider_sync_state.account_id
                               AND c.status = 'connected'
                        )",
                    params![row.0, row.1, owner, lease_expires_at_ms, now],
                )?;
                if changed == 0 {
                    continue;
                }
                let state = mailbox_sync_state_from_parts((
                    row.1.clone(),
                    row.2,
                    row.3,
                    row.4,
                    row.5,
                    Some(owner.to_string()),
                    Some(lease_expires_at_ms),
                    row.6,
                    now,
                ))?;
                claimed.push((row.0, state));
            }
            tx.commit()?;
            Ok(claimed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let rows = tx.query(
                "SELECT s.account_id, s.connection_id, s.provider, s.sync_json,
                        s.next_sync_at_ms, s.last_synced_at_ms,
                        s.created_at_ms, s.updated_at_ms
                   FROM jobs_provider_sync_state AS s
                   JOIN jobs_mailbox_connections AS c
                     ON c.id = s.connection_id AND c.account_id = s.account_id
                  WHERE s.next_sync_at_ms <= $1
                    AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= $1)
                    AND c.status = 'connected'
                  ORDER BY s.next_sync_at_ms ASC, s.connection_id ASC
                  FOR UPDATE OF s SKIP LOCKED
                  LIMIT $2",
                &[&now, &limit],
            )?;
            let mut claimed = Vec::with_capacity(rows.len());
            for row in rows {
                let account_id: String = row.get(0);
                let connection_id: String = row.get(1);
                tx.execute(
                    "UPDATE jobs_provider_sync_state
                        SET lease_owner = $3, lease_expires_at_ms = $4, updated_at_ms = $5
                      WHERE account_id = $1 AND connection_id = $2
                        AND EXISTS (
                            SELECT 1
                              FROM jobs_mailbox_connections AS c
                             WHERE c.id = jobs_provider_sync_state.connection_id
                               AND c.account_id = jobs_provider_sync_state.account_id
                               AND c.status = 'connected'
                        )",
                    &[
                        &account_id,
                        &connection_id,
                        &owner,
                        &lease_expires_at_ms,
                        &now,
                    ],
                )?;
                let state = mailbox_sync_state_from_parts((
                    connection_id,
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    Some(owner.to_string()),
                    Some(lease_expires_at_ms),
                    row.get(6),
                    now,
                ))?;
                claimed.push((account_id, state));
            }
            tx.commit()?;
            Ok(claimed)
        }
    })
}

pub fn finish_mailbox_sync(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
    owner: &str,
    cursor: Value,
    last_error: &str,
    next_sync_at_ms: Option<i64>,
) -> Result<bool> {
    let owner = owner.trim();
    if owner.is_empty() {
        anyhow::bail!("mailbox sync owner is required")
    }
    let now = now_ms();
    let next_sync_at_ms =
        next_sync_at_ms.unwrap_or_else(|| now.saturating_add(MAILBOX_SYNC_DEFAULT_INTERVAL_MS));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row: Option<(String, String, i64, Option<i64>, i64)> = tx
                .query_row(
                    "SELECT provider, sync_json, created_at_ms, last_synced_at_ms, updated_at_ms
                       FROM jobs_provider_sync_state
                      WHERE account_id = ?1 AND connection_id = ?2 AND lease_owner = ?3",
                    params![account_id, connection_id, owner],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((provider, _, created_at_ms, previous_sync, _)) = row else {
                tx.commit()?;
                return Ok(false);
            };
            let state = JobsProviderSyncState {
                connection_id: connection_id.to_string(),
                provider,
                cursor,
                next_sync_at_ms,
                last_synced_at_ms: if last_error.is_empty() {
                    Some(now)
                } else {
                    previous_sync
                },
                last_error: last_error.trim().chars().take(500).collect(),
                lease_owner: None,
                lease_expires_at_ms: None,
                created_at_ms,
                updated_at_ms: now,
            };
            let payload = to_json(&state, "Jobs provider sync state")?;
            let changed = tx.execute(
                "UPDATE jobs_provider_sync_state
                    SET sync_json = ?4, next_sync_at_ms = ?5, last_synced_at_ms = ?6,
                        lease_owner = NULL, lease_expires_at_ms = NULL, updated_at_ms = ?7
                  WHERE account_id = ?1 AND connection_id = ?2 AND lease_owner = ?3",
                params![
                    account_id,
                    connection_id,
                    owner,
                    payload,
                    next_sync_at_ms,
                    state.last_synced_at_ms,
                    now,
                ],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT provider, sync_json, created_at_ms, last_synced_at_ms, updated_at_ms
                   FROM jobs_provider_sync_state
                  WHERE account_id = $1 AND connection_id = $2 AND lease_owner = $3
                  FOR UPDATE",
                &[&account_id, &connection_id, &owner],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(false);
            };
            let previous_sync: Option<i64> = row.get(3);
            let state = JobsProviderSyncState {
                connection_id: connection_id.to_string(),
                provider: row.get(0),
                cursor,
                next_sync_at_ms,
                last_synced_at_ms: if last_error.is_empty() {
                    Some(now)
                } else {
                    previous_sync
                },
                last_error: last_error.trim().chars().take(500).collect(),
                lease_owner: None,
                lease_expires_at_ms: None,
                created_at_ms: row.get(2),
                updated_at_ms: now,
            };
            let payload = to_json(&state, "Jobs provider sync state")?;
            let changed = tx.execute(
                "UPDATE jobs_provider_sync_state
                    SET sync_json = $4, next_sync_at_ms = $5, last_synced_at_ms = $6,
                        lease_owner = NULL, lease_expires_at_ms = NULL, updated_at_ms = $7
                  WHERE account_id = $1 AND connection_id = $2 AND lease_owner = $3",
                &[
                    &account_id,
                    &connection_id,
                    &owner,
                    &payload,
                    &next_sync_at_ms,
                    &state.last_synced_at_ms,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        }
    })
}

pub fn schedule_mailbox_sync_now(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<Option<JobsProviderSyncState>> {
    let mailbox = mailbox_connection(pool, account_id, connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox connection not found"))?;
    if mailbox.status != "connected" {
        anyhow::bail!("mailbox connection is not ready for sync")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| -> Result<()> {
        match pool {
            DbPool::Sqlite(_) => {
                pool.get()?.execute(
                    "UPDATE jobs_provider_sync_state
                    SET next_sync_at_ms = MIN(next_sync_at_ms, ?3), updated_at_ms = ?3
                  WHERE account_id = ?1 AND connection_id = ?2",
                    params![account_id, connection_id, now],
                )?;
                Ok(())
            }
            DbPool::Postgres(_) => {
                pool.get_pg()?.execute(
                    "UPDATE jobs_provider_sync_state
                    SET next_sync_at_ms = LEAST(next_sync_at_ms, $3), updated_at_ms = $3
                  WHERE account_id = $1 AND connection_id = $2",
                    &[&account_id, &connection_id, &now],
                )?;
                Ok(())
            }
        }
    })?;
    mailbox_sync_state(pool, account_id, connection_id)
}

pub fn save_provider_message(
    pool: &DbPool,
    account_id: &str,
    message: &JobsProviderMessage,
) -> Result<(JobsProviderMessage, bool)> {
    let mut value = message.clone();
    value.provider = normalize_mailbox_provider(&value.provider)?;
    if value.connection_id.trim().is_empty()
        || value.external_id.trim().is_empty()
        || value.sender.trim().is_empty()
        || value.received_at_ms <= 0
    {
        anyhow::bail!("provider message is incomplete")
    }
    let mailbox = mailbox_connection(pool, account_id, &value.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox connection not found"))?;
    if mailbox.provider != value.provider {
        anyhow::bail!("provider message does not match the mailbox")
    }
    if let Some(application_id) = value.application_id.as_deref() {
        if get_application(pool, account_id, application_id)?.is_none() {
            anyhow::bail!("application not found")
        }
    }
    value.processing_status = value.processing_status.trim().to_ascii_lowercase();
    if !matches!(
        value.processing_status.as_str(),
        "received" | "processed" | "needs_input" | "ignored" | "failed"
    ) {
        anyhow::bail!("invalid provider message status")
    }
    value.classification = value.classification.trim().to_ascii_lowercase();
    value.confidence = value.confidence.clamp(0.0, 1.0);
    value.recipients = value
        .recipients
        .iter()
        .filter_map(|recipient| normalize_application_email(recipient).ok())
        .collect();
    value.recipients.sort();
    value.recipients.dedup();
    if value.metadata.is_null() {
        value.metadata = json!({});
    }
    let now = now_ms();
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.processing_status != "received" && value.processed_at_ms.is_none() {
        value.processed_at_ms = Some(now);
    }
    let message_hash = private_lookup_hash(
        &format!("jobs-provider-message:{}", value.provider),
        value.external_id.trim(),
    )?;
    let payload = to_json(&value, "Jobs provider message")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let inserted = tx.execute(
                "INSERT INTO jobs_provider_messages (
                    id, account_id, connection_id, provider, provider_message_hash,
                    application_id, processing_status, message_json, received_at_ms,
                    processed_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(account_id, provider, provider_message_hash) DO NOTHING",
                params![
                    value.id,
                    account_id,
                    value.connection_id,
                    value.provider,
                    message_hash,
                    value.application_id,
                    value.processing_status,
                    payload,
                    value.received_at_ms,
                    value.processed_at_ms,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )? > 0;
            let row = tx.query_row(
                "SELECT id, connection_id, provider, application_id, processing_status,
                        message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_provider_messages
                  WHERE account_id = ?1 AND provider = ?2 AND provider_message_hash = ?3",
                params![account_id, value.provider, message_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )?;
            tx.commit()?;
            Ok((
                provider_message_from_parts((
                    row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
                ))?,
                inserted,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let inserted = tx.execute(
                "INSERT INTO jobs_provider_messages (
                    id, account_id, connection_id, provider, provider_message_hash,
                    application_id, processing_status, message_json, received_at_ms,
                    processed_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(account_id, provider, provider_message_hash) DO NOTHING",
                &[
                    &value.id,
                    &account_id,
                    &value.connection_id,
                    &value.provider,
                    &message_hash,
                    &value.application_id,
                    &value.processing_status,
                    &payload,
                    &value.received_at_ms,
                    &value.processed_at_ms,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )? > 0;
            let row = tx.query_one(
                "SELECT id, connection_id, provider, application_id, processing_status,
                        message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_provider_messages
                  WHERE account_id = $1 AND provider = $2 AND provider_message_hash = $3",
                &[&account_id, &value.provider, &message_hash],
            )?;
            let stored = provider_message_from_parts((
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get(6),
                row.get(7),
                row.get(8),
                row.get(9),
            ))?;
            tx.commit()?;
            Ok((stored, inserted))
        }
    })
}

pub fn list_provider_messages(
    pool: &DbPool,
    account_id: &str,
    connection_id: Option<&str>,
    limit: usize,
) -> Result<Vec<JobsProviderMessage>> {
    let limit = limit.clamp(1, MAILBOX_MESSAGE_LIST_MAX) as i64;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut rows = Vec::new();
            if let Some(connection_id) = connection_id {
                let mut stmt = conn.prepare(
                    "SELECT id, connection_id, provider, application_id, processing_status,
                            message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_provider_messages
                      WHERE account_id = ?1 AND connection_id = ?2
                      ORDER BY received_at_ms DESC, id ASC
                      LIMIT ?3",
                )?;
                for row in stmt.query_map(params![account_id, connection_id, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                })? {
                    let row = row?;
                    rows.push(provider_message_from_parts((
                        row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
                    ))?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, connection_id, provider, application_id, processing_status,
                            message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_provider_messages
                      WHERE account_id = ?1
                      ORDER BY received_at_ms DESC, id ASC
                      LIMIT ?2",
                )?;
                for row in stmt.query_map(params![account_id, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                })? {
                    let row = row?;
                    rows.push(provider_message_from_parts((
                        row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
                    ))?);
                }
            }
            Ok(rows)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let rows = if let Some(connection_id) = connection_id {
                conn.query(
                    "SELECT id, connection_id, provider, application_id, processing_status,
                            message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_provider_messages
                      WHERE account_id = $1 AND connection_id = $2
                      ORDER BY received_at_ms DESC, id ASC
                      LIMIT $3",
                    &[&account_id, &connection_id, &limit],
                )?
            } else {
                conn.query(
                    "SELECT id, connection_id, provider, application_id, processing_status,
                            message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_provider_messages
                      WHERE account_id = $1
                      ORDER BY received_at_ms DESC, id ASC
                      LIMIT $2",
                    &[&account_id, &limit],
                )?
            };
            rows.into_iter()
                .map(|row| {
                    provider_message_from_parts((
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                        row.get(5),
                        row.get(6),
                        row.get(7),
                        row.get(8),
                        row.get(9),
                    ))
                })
                .collect()
        }
    })
}

pub fn list_pending_provider_messages(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
    limit: usize,
) -> Result<Vec<JobsProviderMessage>> {
    let limit = limit.clamp(1, MAILBOX_MESSAGE_LIST_MAX) as i64;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, connection_id, provider, application_id, processing_status,
                        message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_provider_messages
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND processing_status = 'received'
                  ORDER BY received_at_ms ASC, id ASC
                  LIMIT ?3",
            )?;
            let mut messages = Vec::new();
            for row in stmt.query_map(params![account_id, connection_id, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })? {
                let row = row?;
                messages.push(provider_message_from_parts((
                    row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
                ))?);
            }
            Ok(messages)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query(
                "SELECT id, connection_id, provider, application_id, processing_status,
                        message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_provider_messages
                  WHERE account_id = $1 AND connection_id = $2
                    AND processing_status = 'received'
                  ORDER BY received_at_ms ASC, id ASC
                  LIMIT $3",
                &[&account_id, &connection_id, &limit],
            )?
            .into_iter()
            .map(|row| {
                provider_message_from_parts((
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    row.get(7),
                    row.get(8),
                    row.get(9),
                ))
            })
            .collect()
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn update_provider_message_processing(
    pool: &DbPool,
    account_id: &str,
    message_id: &str,
    application_id: Option<&str>,
    processing_status: &str,
    classification: &str,
    confidence: f64,
    metadata: Value,
) -> Result<Option<JobsProviderMessage>> {
    let processing_status = processing_status.trim().to_ascii_lowercase();
    if !matches!(
        processing_status.as_str(),
        "received" | "processed" | "needs_input" | "ignored" | "failed"
    ) {
        anyhow::bail!("invalid provider message status")
    }
    let classification = classification.trim().to_ascii_lowercase();
    if classification.len() > 64 {
        anyhow::bail!("provider message classification is too long")
    }
    let application_id = application_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(application_id) = application_id.as_deref() {
        if get_application(pool, account_id, application_id)?.is_none() {
            anyhow::bail!("application not found")
        }
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row: Option<ProviderMessageParts> = tx
                .query_row(
                    "SELECT id, connection_id, provider, application_id, processing_status,
                            message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_provider_messages
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, message_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                            row.get(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let mut message = provider_message_from_parts((
                row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9,
            ))?;
            message.application_id = application_id.clone();
            message.processing_status = processing_status.clone();
            message.classification = classification.clone();
            message.confidence = confidence.clamp(0.0, 1.0);
            message.metadata = if metadata.is_null() {
                json!({})
            } else {
                metadata.clone()
            };
            message.processed_at_ms = if message.processing_status == "received" {
                None
            } else {
                Some(now)
            };
            message.updated_at_ms = now;
            let payload = to_json(&message, "Jobs provider message")?;
            tx.execute(
                "UPDATE jobs_provider_messages
                    SET application_id = ?3, processing_status = ?4, message_json = ?5,
                        processed_at_ms = ?6, updated_at_ms = ?7
                  WHERE account_id = ?1 AND id = ?2",
                params![
                    account_id,
                    message_id,
                    message.application_id,
                    message.processing_status,
                    payload,
                    message.processed_at_ms,
                    now,
                ],
            )?;
            tx.commit()?;
            Ok(Some(message))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT id, connection_id, provider, application_id, processing_status,
                        message_json, received_at_ms, processed_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_provider_messages
                  WHERE account_id = $1 AND id = $2
                  FOR UPDATE",
                &[&account_id, &message_id],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let mut message = provider_message_from_parts((
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get(6),
                row.get(7),
                row.get(8),
                row.get(9),
            ))?;
            message.application_id = application_id;
            message.processing_status = processing_status;
            message.classification = classification;
            message.confidence = confidence.clamp(0.0, 1.0);
            message.metadata = if metadata.is_null() {
                json!({})
            } else {
                metadata
            };
            message.processed_at_ms = if message.processing_status == "received" {
                None
            } else {
                Some(now)
            };
            message.updated_at_ms = now;
            let payload = to_json(&message, "Jobs provider message")?;
            tx.execute(
                "UPDATE jobs_provider_messages
                    SET application_id = $3, processing_status = $4, message_json = $5,
                        processed_at_ms = $6, updated_at_ms = $7
                  WHERE account_id = $1 AND id = $2",
                &[
                    &account_id,
                    &message_id,
                    &message.application_id,
                    &message.processing_status,
                    &payload,
                    &message.processed_at_ms,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok(Some(message))
        }
    })
}
