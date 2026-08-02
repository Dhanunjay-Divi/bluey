const COMMUNICATION_ACTION_PAYLOAD_MAX_BYTES: usize = 64 * 1024;
const COMMUNICATION_ACTION_LIST_MAX: usize = 100;
const COMMUNICATION_ACTION_LEASE_MS: i64 = 60 * 1_000;
const COMMUNICATION_ACTION_MAX_ATTEMPTS: i64 = 5;

type CommunicationActionRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

fn canonical_communication_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(canonical_communication_value)
                .collect(),
        ),
        Value::Object(values) => {
            let sorted = values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_communication_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        _ => value.clone(),
    }
}

fn communication_payload_sha256(payload: &Value) -> Result<String> {
    let encoded = serde_json::to_vec(payload).context("serialize communication action payload")?;
    if encoded.len() > COMMUNICATION_ACTION_PAYLOAD_MAX_BYTES {
        anyhow::bail!("communication action payload is too large")
    }
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn normalize_communication_kind(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if !matches!(value.as_str(), "reply" | "calendar") {
        anyhow::bail!("invalid communication action kind")
    }
    Ok(value)
}

fn normalize_communication_provider(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if !matches!(
        value.as_str(),
        "gmail" | "outlook_email" | "google_calendar" | "outlook_calendar"
    ) {
        anyhow::bail!("invalid communication action provider")
    }
    Ok(value)
}

fn communication_connection_provider(provider: &str) -> &str {
    match provider {
        "google_calendar" => "gmail",
        "outlook_email" | "outlook_calendar" => "outlook",
        value => value,
    }
}

fn communication_payload_text<'a>(payload: &'a Value, key: &str) -> Result<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("communication action payload is missing {key}"))
}

fn validate_communication_payload(kind: &str, payload: &Value) -> Result<()> {
    if !payload.is_object() {
        anyhow::bail!("communication action payload must be an object")
    }
    match kind {
        "reply" => {
            normalize_application_email(communication_payload_text(payload, "to")?)?;
            let subject = communication_payload_text(payload, "subject")?;
            let body = communication_payload_text(payload, "body_text")?;
            if subject.len() > 998 || body.len() > 32_000 {
                anyhow::bail!("communication reply content is too long")
            }
        }
        "calendar" => {
            let title = communication_payload_text(payload, "title")?;
            let starts_at_ms = payload
                .get("starts_at_ms")
                .and_then(Value::as_i64)
                .filter(|value| *value > 0)
                .ok_or_else(|| anyhow::anyhow!("calendar action needs a start time"))?;
            let ends_at_ms = payload
                .get("ends_at_ms")
                .and_then(Value::as_i64)
                .filter(|value| *value > starts_at_ms)
                .ok_or_else(|| anyhow::anyhow!("calendar action needs a valid end time"))?;
            if title.len() > 512 || ends_at_ms.saturating_sub(starts_at_ms) > 24 * 60 * 60 * 1_000
            {
                anyhow::bail!("calendar action duration or title is invalid")
            }
            if let Some(attendees) = payload.get("attendees") {
                let attendees = attendees
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("calendar attendees must be a list"))?;
                if attendees.len() > 25 {
                    anyhow::bail!("calendar action has too many attendees")
                }
                for attendee in attendees {
                    normalize_application_email(attendee.as_str().unwrap_or_default())?;
                }
            }
        }
        _ => anyhow::bail!("invalid communication action kind"),
    }
    Ok(())
}

fn communication_action_from_row(row: CommunicationActionRow) -> Result<JobsCommunicationAction> {
    let mut action: JobsCommunicationAction = parse_json(row.9, "Jobs communication action")?;
    action.id = row.0;
    action.application_id = row.1;
    action.connection_id = row.2;
    action.source_message_id = row.3;
    action.kind = row.4;
    action.provider = row.5;
    action.idempotency_key = row.6;
    action.payload_sha256 = row.7;
    action.status = row.8;
    action.provider_object_id = row.10.unwrap_or_default();
    action.lease_owner = row.11;
    action.lease_expires_at_ms = row.13;
    action.next_attempt_at_ms = row.14;
    action.attempt_count = row.15;
    action.approved_at_ms = row.16;
    action.dispatched_at_ms = row.17;
    action.created_at_ms = row.18;
    action.updated_at_ms = row.19;
    Ok(action)
}

fn sqlite_communication_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommunicationActionRow> {
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
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
        row.get(17)?,
        row.get(18)?,
        row.get(19)?,
    ))
}

fn postgres_communication_row(row: postgres::Row) -> CommunicationActionRow {
    (
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
        row.get(10),
        row.get(11),
        row.get(12),
        row.get(13),
        row.get(14),
        row.get(15),
        row.get(16),
        row.get(17),
        row.get(18),
        row.get(19),
    )
}

const COMMUNICATION_ACTION_SELECT: &str =
    "id, application_id, connection_id, source_message_id, kind, provider,
     idempotency_key, payload_sha256, status, action_json, provider_object_id,
     lease_owner, fence, lease_expires_at_ms, next_attempt_at_ms, attempt_count,
     approved_at_ms, dispatched_at_ms, created_at_ms, updated_at_ms";

pub fn communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
) -> Result<Option<JobsCommunicationAction>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let row = conn
                .query_row(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}
                           FROM jobs_communication_actions
                          WHERE account_id = ?1 AND id = ?2"
                    ),
                    params![account_id, action_id],
                    sqlite_communication_row,
                )
                .optional()?;
            row.map(communication_action_from_row).transpose()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query_opt(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &action_id],
            )?
            .map(postgres_communication_row)
            .map(communication_action_from_row)
            .transpose()
        }
    })
}

pub fn list_communication_actions(
    pool: &DbPool,
    account_id: &str,
    application_id: Option<&str>,
    limit: usize,
) -> Result<Vec<JobsCommunicationAction>> {
    let limit = limit.clamp(1, COMMUNICATION_ACTION_LIST_MAX) as i64;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut actions = Vec::new();
            if let Some(application_id) = application_id {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND application_id = ?2
                      ORDER BY created_at_ms DESC, id ASC LIMIT ?3"
                ))?;
                for row in stmt.query_map(
                    params![account_id, application_id, limit],
                    sqlite_communication_row,
                )? {
                    actions.push(communication_action_from_row(row?)?);
                }
            } else {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1
                      ORDER BY created_at_ms DESC, id ASC LIMIT ?2"
                ))?;
                for row in
                    stmt.query_map(params![account_id, limit], sqlite_communication_row)?
                {
                    actions.push(communication_action_from_row(row?)?);
                }
            }
            Ok(actions)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let rows = if let Some(application_id) = application_id {
                conn.query(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}
                           FROM jobs_communication_actions
                          WHERE account_id = $1 AND application_id = $2
                          ORDER BY created_at_ms DESC, id ASC LIMIT $3"
                    ),
                    &[&account_id, &application_id, &limit],
                )?
            } else {
                conn.query(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}
                           FROM jobs_communication_actions
                          WHERE account_id = $1
                          ORDER BY created_at_ms DESC, id ASC LIMIT $2"
                    ),
                    &[&account_id, &limit],
                )?
            };
            rows.into_iter()
                .map(postgres_communication_row)
                .map(communication_action_from_row)
                .collect()
        }
    })
}

fn validate_communication_relationships(
    pool: &DbPool,
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<()> {
    if get_application(pool, account_id, &action.application_id)?.is_none() {
        anyhow::bail!("application not found")
    }
    let connection = mailbox_connection(pool, account_id, &action.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("mailbox connection not found"))?;
    if connection.status != "connected"
        || connection.provider != communication_connection_provider(&action.provider)
    {
        anyhow::bail!("communication action does not match a connected mailbox")
    }
    if let Some(message_id) = action.source_message_id.as_deref() {
        let found = crate::db::run_blocking_db(|| -> Result<bool> {
            match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let found = conn
                    .query_row(
                        "SELECT 1 FROM jobs_provider_messages
                          WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                            AND application_id = ?4",
                        params![
                            account_id,
                            action.connection_id,
                            message_id,
                            action.application_id,
                        ],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                Ok(found)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                Ok(conn
                    .query_opt(
                        "SELECT 1 FROM jobs_provider_messages
                          WHERE account_id = $1 AND connection_id = $2 AND id = $3
                            AND application_id = $4",
                        &[
                            &account_id,
                            &action.connection_id,
                            &message_id,
                            &action.application_id,
                        ],
                    )?
                    .is_some())
            }
            }
        })?;
        if !found {
            anyhow::bail!("source mailbox message not found")
        }
    }
    Ok(())
}

pub fn create_communication_action(
    pool: &DbPool,
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<(JobsCommunicationAction, bool)> {
    let mut value = action.clone();
    value.kind = normalize_communication_kind(&value.kind)?;
    value.provider = normalize_communication_provider(&value.provider)?;
    value.source_message_id = value
        .source_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if value.kind == "reply" && value.source_message_id.is_none() {
        anyhow::bail!("communication replies require a source mailbox message")
    }
    value.idempotency_key = value.idempotency_key.trim().to_string();
    if value.application_id.trim().is_empty()
        || value.connection_id.trim().is_empty()
        || value.idempotency_key.is_empty()
        || value.idempotency_key.len() > 240
        || value.idempotency_key.bytes().any(|byte| byte.is_ascii_control())
    {
        anyhow::bail!("communication action is incomplete")
    }
    value.payload = canonical_communication_value(&value.payload);
    validate_communication_payload(&value.kind, &value.payload)?;
    value.payload_sha256 = communication_payload_sha256(&value.payload)?;
    value.status = "awaiting_approval".to_string();
    value.provider_object_id.clear();
    value.lease_owner = None;
    value.lease_expires_at_ms = None;
    value.attempt_count = 0;
    value.approved_at_ms = None;
    value.dispatched_at_ms = None;
    let now = now_ms();
    value.id = if value.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        value.id.trim().to_string()
    };
    value.next_attempt_at_ms = now;
    value.created_at_ms = now;
    value.updated_at_ms = now;
    validate_communication_relationships(pool, account_id, &value)?;
    let payload = to_json(&value, "Jobs communication action")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let inserted = tx.execute(
                "INSERT INTO jobs_communication_actions (
                    id, account_id, application_id, connection_id, source_message_id,
                    kind, provider, idempotency_key, payload_sha256, status, action_json,
                    next_attempt_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(account_id, idempotency_key) DO NOTHING",
                params![
                    value.id,
                    account_id,
                    value.application_id,
                    value.connection_id,
                    value.source_message_id,
                    value.kind,
                    value.provider,
                    value.idempotency_key,
                    value.payload_sha256,
                    value.status,
                    payload,
                    value.next_attempt_at_ms,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )? > 0;
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND idempotency_key = ?2"
                ),
                params![account_id, value.idempotency_key],
                sqlite_communication_row,
            )?;
            tx.commit()?;
            let stored = communication_action_from_row(row)?;
            if stored.payload_sha256 != value.payload_sha256
                || stored.application_id != value.application_id
                || stored.connection_id != value.connection_id
                || stored.kind != value.kind
                || stored.provider != value.provider
            {
                anyhow::bail!("communication action idempotency key was reused")
            }
            Ok((stored, inserted))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let inserted = tx.execute(
                "INSERT INTO jobs_communication_actions (
                    id, account_id, application_id, connection_id, source_message_id,
                    kind, provider, idempotency_key, payload_sha256, status, action_json,
                    next_attempt_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                 ON CONFLICT(account_id, idempotency_key) DO NOTHING",
                &[
                    &value.id,
                    &account_id,
                    &value.application_id,
                    &value.connection_id,
                    &value.source_message_id,
                    &value.kind,
                    &value.provider,
                    &value.idempotency_key,
                    &value.payload_sha256,
                    &value.status,
                    &payload,
                    &value.next_attempt_at_ms,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )? > 0;
            let row = tx.query_one(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND idempotency_key = $2"
                ),
                &[&account_id, &value.idempotency_key],
            )?;
            tx.commit()?;
            let stored = communication_action_from_row(postgres_communication_row(row))?;
            if stored.payload_sha256 != value.payload_sha256
                || stored.application_id != value.application_id
                || stored.connection_id != value.connection_id
                || stored.kind != value.kind
                || stored.provider != value.provider
            {
                anyhow::bail!("communication action idempotency key was reused")
            }
            Ok((stored, inserted))
        }
    })
}

fn transition_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
    approve: bool,
) -> Result<Option<JobsCommunicationAction>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let (status, approved_at_ms) = if approve {
                ("approved", Some(now))
            } else {
                ("cancelled", None)
            };
            conn.execute(
                "UPDATE jobs_communication_actions
                    SET status = ?3, approved_at_ms = COALESCE(?4, approved_at_ms),
                        updated_at_ms = ?5
                  WHERE account_id = ?1 AND id = ?2
                    AND status IN ('awaiting_approval', 'needs_input')",
                params![account_id, action_id, status, approved_at_ms, now],
            )?;
            conn.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )
            .optional()?
            .map(communication_action_from_row)
            .transpose()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let (status, approved_at_ms) = if approve {
                ("approved", Some(now))
            } else {
                ("cancelled", None)
            };
            conn.execute(
                "UPDATE jobs_communication_actions
                    SET status = $3, approved_at_ms = COALESCE($4, approved_at_ms),
                        updated_at_ms = $5
                  WHERE account_id = $1 AND id = $2
                    AND status IN ('awaiting_approval', 'needs_input')",
                &[&account_id, &action_id, &status, &approved_at_ms, &now],
            )?;
            conn.query_opt(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &action_id],
            )?
            .map(postgres_communication_row)
            .map(communication_action_from_row)
            .transpose()
        }
    })
}

pub fn approve_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
) -> Result<Option<JobsCommunicationAction>> {
    let action = transition_communication_action(pool, account_id, action_id, true)?;
    if let Some(action) = action.as_ref() {
        if !matches!(action.status.as_str(), "approved" | "dispatching") {
            anyhow::bail!("communication action cannot be approved in its current state")
        }
    }
    Ok(action)
}

pub fn cancel_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
) -> Result<Option<JobsCommunicationAction>> {
    let action = transition_communication_action(pool, account_id, action_id, false)?;
    if let Some(action) = action.as_ref() {
        if action.status != "cancelled" {
            anyhow::bail!("communication action cannot be cancelled after dispatch")
        }
    }
    Ok(action)
}

pub fn claim_communication_action(
    pool: &DbPool,
    owner_id: &str,
) -> Result<Option<JobsCommunicationActionLease>> {
    if owner_id.trim().is_empty() || owner_id.len() > 240 {
        anyhow::bail!("communication worker identity is invalid")
    }
    let now = now_ms();
    let expires_at = now.saturating_add(COMMUNICATION_ACTION_LEASE_MS);
    let lease_token = random_execution_lease_token();
    let token_hash = execution_lease_token_hash(&lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'side_effect_unknown', updated_at_ms = ?1
                  WHERE status = 'dispatching' AND lease_expires_at_ms <= ?1",
                params![now],
            )?;
            let candidate: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT a.id, a.account_id, a.fence
                       FROM jobs_communication_actions a
                       JOIN jobs_mailbox_connections c
                         ON c.id = a.connection_id
                        AND c.account_id = a.account_id
                        AND c.status = 'connected'
                      WHERE a.status IN ('approved', 'failed')
                        AND a.next_attempt_at_ms <= ?1 AND a.attempt_count < ?2
                        AND (
                          (a.provider IN ('gmail', 'google_calendar') AND c.provider = 'gmail')
                          OR
                          (a.provider IN ('outlook_email', 'outlook_calendar')
                            AND c.provider = 'outlook')
                        )
                      ORDER BY a.next_attempt_at_ms ASC, a.created_at_ms ASC LIMIT 1",
                    params![now, COMMUNICATION_ACTION_MAX_ATTEMPTS],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((action_id, account_id, fence)) = candidate else {
                tx.commit()?;
                return Ok(None);
            };
            let fence = fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'dispatching', lease_owner = ?2, lease_token_sha256 = ?3,
                        fence = ?4, lease_expires_at_ms = ?5, attempt_count = attempt_count + 1,
                        dispatched_at_ms = COALESCE(dispatched_at_ms, ?1), updated_at_ms = ?1
                  WHERE id = ?6 AND account_id = ?7",
                params![now, owner_id, token_hash, fence, expires_at, action_id, account_id],
            )?;
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            tx.commit()?;
            Ok(Some(JobsCommunicationActionLease {
                account_id,
                action: communication_action_from_row(row)?,
                lease_token,
                fence,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'side_effect_unknown', updated_at_ms = $1
                  WHERE status = 'dispatching' AND lease_expires_at_ms <= $1",
                &[&now],
            )?;
            let candidate = tx.query_opt(
                "SELECT a.id, a.account_id, a.fence
                   FROM jobs_communication_actions a
                   JOIN jobs_mailbox_connections c
                     ON c.id = a.connection_id
                    AND c.account_id = a.account_id
                    AND c.status = 'connected'
                  WHERE a.status IN ('approved', 'failed')
                    AND a.next_attempt_at_ms <= $1 AND a.attempt_count < $2
                    AND (
                      (a.provider IN ('gmail', 'google_calendar') AND c.provider = 'gmail')
                      OR
                      (a.provider IN ('outlook_email', 'outlook_calendar')
                        AND c.provider = 'outlook')
                    )
                  ORDER BY a.next_attempt_at_ms ASC, a.created_at_ms ASC
                  FOR UPDATE OF a SKIP LOCKED LIMIT 1",
                &[&now, &COMMUNICATION_ACTION_MAX_ATTEMPTS],
            )?;
            let Some(candidate) = candidate else {
                tx.commit()?;
                return Ok(None);
            };
            let action_id: String = candidate.get(0);
            let account_id: String = candidate.get(1);
            let fence: i64 = candidate
                .get::<_, i64>(2)
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            let row = tx.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET status = 'dispatching', lease_owner = $2, lease_token_sha256 = $3,
                            fence = $4, lease_expires_at_ms = $5,
                            attempt_count = attempt_count + 1,
                            dispatched_at_ms = COALESCE(dispatched_at_ms, $1),
                            updated_at_ms = $1
                      WHERE id = $6 AND account_id = $7
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &now,
                    &owner_id,
                    &token_hash,
                    &fence,
                    &expires_at,
                    &action_id,
                    &account_id,
                ],
            )?;
            tx.commit()?;
            Ok(Some(JobsCommunicationActionLease {
                account_id,
                action: communication_action_from_row(postgres_communication_row(row))?,
                lease_token,
                fence,
            }))
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn finish_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
    owner_id: &str,
    lease_token: &str,
    fence: i64,
    outcome: &str,
    provider_object_id: Option<&str>,
) -> Result<JobsCommunicationAction> {
    let outcome = outcome.trim().to_ascii_lowercase();
    if !matches!(
        outcome.as_str(),
        "sent" | "calendar_created" | "failed" | "needs_input" | "side_effect_unknown"
    ) {
        anyhow::bail!("invalid communication action outcome")
    }
    let provider_object_id = provider_object_id.unwrap_or_default().trim();
    if matches!(outcome.as_str(), "sent" | "calendar_created") && provider_object_id.is_empty() {
        anyhow::bail!("successful communication action needs provider evidence")
    }
    let now = now_ms();
    let next_attempt_at = if outcome == "failed" {
        now.saturating_add(60 * 1_000)
    } else {
        now
    };
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let stored: Option<String> = conn
                .query_row(
                    "SELECT lease_token_sha256 FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2 AND lease_owner = ?3 AND fence = ?4
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    params![account_id, action_id, owner_id, fence],
                    |row| row.get(0),
                )
                .optional()?;
            if !stored
                .as_deref()
                .is_some_and(|stored| execution_lease_token_matches(stored, lease_token))
            {
                anyhow::bail!("communication action lease not found")
            }
            conn.execute(
                "UPDATE jobs_communication_actions
                    SET status = ?5, provider_object_id = NULLIF(?6, ''),
                        next_attempt_at_ms = ?7, lease_owner = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        updated_at_ms = ?8
                  WHERE account_id = ?1 AND id = ?2 AND lease_owner = ?3 AND fence = ?4",
                params![
                    account_id,
                    action_id,
                    owner_id,
                    fence,
                    outcome,
                    provider_object_id,
                    next_attempt_at,
                    now,
                ],
            )?;
            let row = conn.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            communication_action_from_row(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let stored = conn.query_opt(
                "SELECT lease_token_sha256 FROM jobs_communication_actions
                  WHERE account_id = $1 AND id = $2 AND lease_owner = $3 AND fence = $4
                    AND status IN ('dispatching', 'side_effect_unknown')",
                &[&account_id, &action_id, &owner_id, &fence],
            )?;
            let stored: Option<String> = stored.map(|row| row.get(0));
            if !stored
                .as_deref()
                .is_some_and(|stored| execution_lease_token_matches(stored, lease_token))
            {
                anyhow::bail!("communication action lease not found")
            }
            let row = conn.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET status = $5, provider_object_id = NULLIF($6, ''),
                            next_attempt_at_ms = $7, lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            updated_at_ms = $8
                      WHERE account_id = $1 AND id = $2 AND lease_owner = $3 AND fence = $4
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &account_id,
                    &action_id,
                    &owner_id,
                    &fence,
                    &outcome,
                    &provider_object_id,
                    &next_attempt_at,
                    &now,
                ],
            )?;
            communication_action_from_row(postgres_communication_row(row))
        }
    })
}
