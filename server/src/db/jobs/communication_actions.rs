const COMMUNICATION_ACTION_PAYLOAD_MAX_BYTES: usize = 64 * 1024;
const COMMUNICATION_ACTION_LIST_MAX: usize = 100;
const COMMUNICATION_ACTION_LEASE_MS: i64 = 60 * 1_000;
const COMMUNICATION_ACTION_MAX_ATTEMPTS: i64 = 5;
pub(crate) const COMMUNICATION_ACTION_REVISION_MAX: i64 = 9_007_199_254_740_991;
const COMMUNICATION_ACTION_REVISION_LIFECYCLE_HEADROOM: i64 = 48;
const COMMUNICATION_RECONCILIATION_ABSENCE_THRESHOLD: i64 = 3;
const COMMUNICATION_RECONCILIATION_ABSENCE_MIN_AGE_MS: i64 = 15 * 60 * 1_000;

type CommunicationOperationalHoldScanCursor = (i64, i64, String);

static COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS: std::sync::LazyLock<
    std::sync::Mutex<Option<CommunicationOperationalHoldScanCursor>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

pub(crate) fn communication_review_text_is_safe(value: &str) -> bool {
    value.chars().all(|character| {
        !matches!(
            get_general_category(character),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::Surrogate
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
        )
    })
}

pub(crate) fn communication_body_text_is_safe(value: &str) -> bool {
    value
        .chars()
        .all(|character| match get_general_category(character) {
            GeneralCategory::Control => matches!(character, '\t' | '\n' | '\r'),
            GeneralCategory::Format => matches!(character, '\u{200c}' | '\u{200d}'),
            GeneralCategory::Surrogate
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator => false,
            _ => true,
        })
}

fn exact_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn communication_flag_enabled(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn require_communication_write_unfenced_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    connection_id: &str,
) -> Result<()> {
    let fenced: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM jobs_communication_write_fences
             WHERE account_id = ?1 AND connection_id IN ('', ?2)
         )",
        params![account_id, connection_id],
        |row| row.get(0),
    )?;
    if fenced {
        anyhow::bail!("communication writes are draining")
    }
    Ok(())
}

fn require_communication_write_unfenced_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    connection_id: &str,
) -> Result<()> {
    let fenced: bool = tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_communication_write_fences
                 WHERE account_id = $1 AND connection_id IN ('', $2)
             )",
            &[&account_id, &connection_id],
        )?
        .get(0);
    if fenced {
        anyhow::bail!("communication writes are draining")
    }
    Ok(())
}

fn communication_write_is_fenced(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT EXISTS(
                SELECT 1 FROM jobs_communication_write_fences
                 WHERE account_id = ?1 AND connection_id IN ('', ?2)
             )",
                params![account_id, connection_id],
                |row| row.get(0),
            )
            .map_err(Into::into),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_communication_write_fences
                     WHERE account_id = $1 AND connection_id IN ('', $2)
                 )",
                &[&account_id, &connection_id],
            )?
            .get(0)),
    })
}

fn communication_dispatch_is_held_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    mailbox_provider: &str,
) -> Result<bool> {
    let employer_domain = communication_employer_domain_sqlite_tx(tx, account_id, application_id)?;
    communication_dispatch_is_held_sqlite_tx_after_authority(
        tx,
        account_id,
        application_id,
        mailbox_provider,
        &employer_domain,
    )
}

fn communication_dispatch_is_held_sqlite_tx_after_authority(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    mailbox_provider: &str,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<bool> {
    let mut context = operational_hold_context_for_application_sqlite_tx_after_authority(
        tx,
        account_id,
        application_id,
        employer_domain,
        None,
        None,
        None,
    )
    .map_err(anyhow::Error::new)?;
    context
        .insert_scope(OperationalHoldScopeKind::MailboxProvider, mailbox_provider)
        .map_err(anyhow::Error::new)?;
    Ok(matches!(
        evaluate_operational_capability_sqlite_tx(
            tx,
            OperationalCapability::CommunicationDispatch,
            &context,
        )
        .map_err(anyhow::Error::new)?,
        OperationalCapabilityEvaluation::Held(_)
    ))
}

fn communication_dispatch_is_held_postgres_tx_after_authority_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    mailbox_provider: &str,
) -> Result<bool> {
    let employer_domain = communication_employer_domain_postgres_tx_after_authority_prelock(
        tx,
        account_id,
        application_id,
    )?;
    communication_dispatch_is_held_postgres_tx_with_domain_after_authority_prelock(
        tx,
        account_id,
        application_id,
        mailbox_provider,
        &employer_domain,
    )
}

fn communication_dispatch_is_held_postgres_tx_with_domain_after_authority_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    mailbox_provider: &str,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<bool> {
    let mut context = operational_hold_context_for_application_postgres_tx_after_authority_prelock(
        tx,
        account_id,
        application_id,
        employer_domain,
        None,
        None,
        None,
    )
    .map_err(anyhow::Error::new)?;
    context
        .insert_scope(OperationalHoldScopeKind::MailboxProvider, mailbox_provider)
        .map_err(anyhow::Error::new)?;
    Ok(matches!(
        evaluate_operational_capability_postgres_tx_after_authority_prelock(
            tx,
            OperationalCapability::CommunicationDispatch,
            &context,
        )
        .map_err(anyhow::Error::new)?,
        OperationalCapabilityEvaluation::Held(_)
    ))
}

fn communication_employer_domain_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<OperationalHoldEmployerDomain> {
    let (stored_application_id, job_id, application_json): (String, String, String) = tx
        .query_row(
            "SELECT id, job_id, application_json FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("communication application is unavailable"))?;
    let application = parse_application_json(
        application_json,
        &stored_application_id,
        &job_id,
        "communication application",
    )?;
    submitted_execution_employer_domain(account_id, &application)
}

fn communication_employer_domain_postgres_tx_after_authority_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<OperationalHoldEmployerDomain> {
    let row = tx
        .query_opt(
            "SELECT id, job_id, application_json FROM jobs_applications AS application
              WHERE account_id = $1 AND id = $2 FOR SHARE OF application",
            &[&account_id, &application_id],
        )?
        .ok_or_else(|| anyhow::anyhow!("communication application is unavailable"))?;
    let stored_application_id: String = row.get(0);
    let job_id: String = row.get(1);
    let application = parse_application_json(
        row.get(2),
        &stored_application_id,
        &job_id,
        "communication application",
    )?;
    submitted_execution_employer_domain(account_id, &application)
}

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
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
    String,
    i64,
    String,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
    i64,
);

type CommunicationReconciliationCandidateRow = (
    String,
    String,
    String,
    i64,
    String,
    i64,
    String,
    i64,
    String,
);

fn canonical_communication_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.iter().map(canonical_communication_value).collect())
        }
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
    let encoded = serde_json::to_vec(&canonical_communication_value(payload))
        .context("serialize communication action payload")?;
    if encoded.len() > COMMUNICATION_ACTION_PAYLOAD_MAX_BYTES {
        anyhow::bail!("communication action payload is too large")
    }
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn communication_authority_sha256(
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<String> {
    let authority = serde_json::json!({
        "account_id": account_id,
        "application_id": action.application_id,
        "connection_id": action.connection_id,
        "source_message_id": action.source_message_id,
        "kind": action.kind,
        "provider": action.provider,
        "payload": action.payload,
    });
    let encoded = serde_json::to_vec(&canonical_communication_value(&authority))
        .context("serialize communication action authority")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn communication_evidence_sha256(evidence: &Value) -> Result<String> {
    let encoded = serde_json::to_vec(&canonical_communication_value(evidence))
        .context("serialize communication evidence")?;
    if encoded.len() > COMMUNICATION_ACTION_PAYLOAD_MAX_BYTES {
        anyhow::bail!("communication action evidence is too large")
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

fn validate_communication_kind_provider(kind: &str, provider: &str) -> Result<()> {
    let valid = matches!(
        (kind, provider),
        ("reply", "gmail")
            | ("reply", "outlook_email")
            | ("calendar", "google_calendar")
            | ("calendar", "outlook_calendar")
    );
    if !valid {
        anyhow::bail!("communication action kind and provider do not match")
    }
    Ok(())
}

fn communication_required_grant(provider: &str) -> Result<(&'static str, &'static str)> {
    match provider {
        "gmail" => Ok((
            "recruiter_reply",
            "https://www.googleapis.com/auth/gmail.send",
        )),
        "google_calendar" => Ok((
            "interview_calendar",
            "https://www.googleapis.com/auth/calendar.events",
        )),
        "outlook_email" => Ok(("recruiter_reply", "Mail.Send")),
        "outlook_calendar" => Ok(("interview_calendar", "Calendars.ReadWrite")),
        _ => anyhow::bail!("invalid communication action provider"),
    }
}

pub fn communication_grant_sha256(credential: &JobsProviderCredential) -> Result<String> {
    let mut scopes = credential.scopes.clone();
    scopes.sort();
    scopes.dedup();
    let mut capabilities = credential.capabilities.clone();
    capabilities.sort();
    capabilities.dedup();
    let value = serde_json::json!({
        "connection_id": credential.connection_id,
        "provider": credential.provider,
        "provider_subject": credential.provider_subject,
        "grant_revision": credential.grant_revision,
        "scopes": scopes,
        "capabilities": capabilities,
    });
    let bytes = serde_json::to_vec(&canonical_communication_value(&value))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_communication_grant(
    action_provider: &str,
    mailbox: &MailboxConnection,
    credential: &JobsProviderCredential,
) -> Result<(i64, String)> {
    let connection_provider = communication_connection_provider(action_provider);
    let (capability, scope) = communication_required_grant(action_provider)?;
    if mailbox.status != "connected"
        || mailbox.provider != connection_provider
        || credential.connection_id != mailbox.id
        || credential.provider != connection_provider
        || credential.grant_revision <= 0
        || !mailbox.capabilities.iter().any(|value| value == capability)
        || !credential
            .capabilities
            .iter()
            .any(|value| value == capability)
        || !credential.scopes.iter().any(|value| value == scope)
    {
        anyhow::bail!("communication action needs an exact provider write grant")
    }
    let computed = communication_grant_sha256(credential)?;
    if credential.grant_sha256.len() != 64 || credential.grant_sha256 != computed {
        anyhow::bail!("communication provider grant digest is invalid")
    }
    Ok((credential.grant_revision, computed))
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
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("communication action payload is missing {key}"))
}

fn normalize_communication_email(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 320
        || !communication_review_text_is_safe(&normalized)
        || normalized
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        anyhow::bail!("communication action email address is invalid")
    }
    let mut pieces = normalized.split('@');
    let local = pieces.next().unwrap_or_default();
    let domain = pieces.next().unwrap_or_default();
    if local.is_empty()
        || domain.is_empty()
        || pieces.next().is_some()
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
        || !domain.contains('.')
    {
        anyhow::bail!("communication action email address is invalid")
    }
    Ok(normalized)
}

fn normalize_communication_payload_emails(kind: &str, payload: &mut Value) -> Result<()> {
    let object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("communication action payload must be an object"))?;
    match kind {
        "reply" => {
            if let Some(to) = object.get("to").and_then(Value::as_str) {
                object.insert(
                    "to".to_string(),
                    Value::String(normalize_communication_email(to)?),
                );
            }
        }
        "calendar" => {
            if let Some(attendees) = object.get_mut("attendees").and_then(Value::as_array_mut) {
                for attendee in attendees {
                    let value = attendee.as_str().unwrap_or_default();
                    *attendee = Value::String(normalize_communication_email(value)?);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn communication_source_metadata_text<'a>(
    message: &'a JobsProviderMessage,
    key: &str,
) -> Option<&'a str> {
    message
        .metadata
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 2_048
                && *value == value.trim()
                && communication_review_text_is_safe(value)
        })
}

pub(crate) fn communication_source_reply_target(
    action_provider: &str,
    message: &JobsProviderMessage,
) -> Result<String> {
    match action_provider {
        "gmail" | "outlook_email" => {
            let target = communication_source_metadata_text(message, "reply_target")
                .ok_or_else(|| anyhow::anyhow!("communication reply target is unavailable"))?;
            let normalized = normalize_communication_email(target)?;
            if normalized != target {
                anyhow::bail!("communication reply target is not canonical")
            }
            Ok(normalized)
        }
        _ => anyhow::bail!("communication reply provider is invalid"),
    }
}

pub(crate) fn validate_communication_reply_source(
    action: &JobsCommunicationAction,
    message: &JobsProviderMessage,
) -> Result<()> {
    if action.kind != "reply" {
        return Ok(());
    }
    let expected_provider = communication_connection_provider(&action.provider);
    if message.provider != expected_provider
        || message.connection_id != action.connection_id
        || message.application_id.as_deref() != Some(action.application_id.as_str())
    {
        anyhow::bail!("communication reply source provider does not match its authority")
    }
    let recipient =
        normalize_communication_email(communication_payload_text(&action.payload, "to")?)?;
    let reply_target = communication_source_reply_target(&action.provider, message)?;
    if recipient != reply_target {
        anyhow::bail!("communication reply recipient does not match the source reply target")
    }
    match action.provider.as_str() {
        "gmail" if communication_source_metadata_text(message, "thread_id").is_some() => {}
        "outlook_email"
            if communication_source_metadata_text(message, "provider_id").is_some()
                && communication_source_metadata_text(message, "conversation_id").is_some()
                && (communication_source_metadata_text(message, "rfc_message_id").is_some()
                    || (message.external_id.starts_with('<')
                        && message.external_id.ends_with('>')
                        && message.external_id.len() <= 998
                        && !message
                            .external_id
                            .bytes()
                            .any(|byte| byte.is_ascii_control()))) => {}
        _ => anyhow::bail!("communication reply source metadata is incomplete"),
    }
    Ok(())
}

fn validate_communication_payload(kind: &str, payload: &Value) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("communication action payload must be an object"))?;
    match kind {
        "reply" => {
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "to" | "subject" | "body_text"))
            {
                anyhow::bail!("communication reply payload has unsupported fields")
            }
            let to = communication_payload_text(payload, "to")?;
            if normalize_communication_email(to)? != to {
                anyhow::bail!("communication reply recipient is not canonical")
            }
            let subject = communication_payload_text(payload, "subject")?;
            let body = communication_payload_text(payload, "body_text")?;
            if subject.len() > 998
                || subject != subject.trim()
                || !communication_review_text_is_safe(subject)
                || body.len() > 32_000
                || !communication_body_text_is_safe(body)
            {
                anyhow::bail!("communication reply content is too long")
            }
        }
        "calendar" => {
            if object.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "title" | "starts_at_ms" | "ends_at_ms" | "attendees" | "time_zone"
                )
            }) {
                anyhow::bail!("calendar action payload has unsupported fields")
            }
            let title = communication_payload_text(payload, "title")?;
            let time_zone = communication_payload_text(payload, "time_zone")?;
            let _: chrono_tz::Tz = time_zone
                .parse()
                .map_err(|_| anyhow::anyhow!("calendar action needs a valid IANA time zone"))?;
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
            if chrono::DateTime::<chrono::Utc>::from_timestamp_millis(starts_at_ms).is_none()
                || chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ends_at_ms).is_none()
            {
                anyhow::bail!("calendar action time is outside the supported range")
            }
            if title.len() > 512
                || title != title.trim()
                || !communication_review_text_is_safe(title)
                || time_zone.len() > 64
                || time_zone != time_zone.trim()
                || time_zone.bytes().any(|byte| byte.is_ascii_control())
                || ends_at_ms.saturating_sub(starts_at_ms) > 24 * 60 * 60 * 1_000
            {
                anyhow::bail!("calendar action duration or title is invalid")
            }
            let attendees = payload
                .get("attendees")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("calendar attendees must be a list"))?;
            if attendees.len() > 25 {
                anyhow::bail!("calendar action has too many attendees")
            }
            let mut unique_attendees = BTreeSet::new();
            for attendee in attendees {
                let attendee = attendee.as_str().unwrap_or_default();
                if normalize_communication_email(attendee)? != attendee {
                    anyhow::bail!("calendar attendee is not canonical")
                }
                if !unique_attendees.insert(attendee) {
                    anyhow::bail!("calendar action has duplicate attendees")
                }
            }
        }
        _ => anyhow::bail!("invalid communication action kind"),
    }
    Ok(())
}

fn communication_action_from_row(row: CommunicationActionRow) -> Result<JobsCommunicationAction> {
    let mut action: JobsCommunicationAction = parse_json(row.10, "Jobs communication action")?;
    action.id = row.0;
    action.application_id = row.1;
    action.connection_id = row.2;
    action.source_message_id = row.3;
    action.kind = row.4;
    action.provider = row.5;
    action.idempotency_key = row.6;
    action.payload_sha256 = row.7;
    action.authority_sha256 = row.8;
    action.status = row.9;
    action.provider_object_id = row.11.unwrap_or_default();
    action.lease_owner = row.12;
    action.lease_kind = row.13;
    action.lease_expires_at_ms = row.15;
    action.active_attempt_id = row.16;
    action.next_attempt_at_ms = row.17;
    action.attempt_count = row.18;
    action.reconciliation_count = row.19;
    action.approval_revision = row.20;
    action.approved_authority_sha256 = row.21;
    action.approved_grant_revision = row.22;
    action.approved_grant_sha256 = row.23;
    action.approved_at_ms = row.24;
    action.dispatched_at_ms = row.25;
    action.created_at_ms = row.26;
    action.updated_at_ms = row.27;
    action.action_revision = row.28;
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
        row.get(20)?,
        row.get(21)?,
        row.get(22)?,
        row.get(23)?,
        row.get(24)?,
        row.get(25)?,
        row.get(26)?,
        row.get(27)?,
        row.get(28)?,
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
        row.get(20),
        row.get(21),
        row.get(22),
        row.get(23),
        row.get(24),
        row.get(25),
        row.get(26),
        row.get(27),
        row.get(28),
    )
}

const COMMUNICATION_ACTION_SELECT: &str =
    "id, application_id, connection_id, source_message_id, kind, provider,
     idempotency_key, payload_sha256, authority_sha256, status, action_json,
     provider_object_id, lease_owner, lease_kind, fence, lease_expires_at_ms,
     active_attempt_id, next_attempt_at_ms, attempt_count, reconciliation_count,
     approval_revision, approved_authority_sha256, approved_grant_revision,
     approved_grant_sha256, approved_at_ms, dispatched_at_ms, created_at_ms,
     updated_at_ms, action_revision";

/// Return one transaction-owned clock sample after the caller has acquired the
/// complete row-lock set for a communication lease mutation. A process clock
/// captured before `run_blocking_db` is discovery-only and must never authorize
/// a lease, provider dispatch, or reconciliation mutation.
fn communication_post_lock_db_now_postgres_tx(tx: &mut postgres::Transaction<'_>) -> Result<i64> {
    let now_ms: i64 = tx
        .query_one(
            "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
            &[],
        )?
        .get(0);
    if !(0..=COMMUNICATION_ACTION_REVISION_MAX).contains(&now_ms) {
        anyhow::bail!("communication database time is outside the supported range")
    }
    Ok(now_ms)
}

fn require_communication_lease_current_at_ms(
    action: &JobsCommunicationAction,
    now_ms: i64,
) -> Result<()> {
    if action
        .lease_expires_at_ms
        .is_none_or(|expires_at_ms| expires_at_ms <= now_ms)
    {
        anyhow::bail!("communication action lease not found")
    }
    Ok(())
}

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
                for row in stmt.query_map(params![account_id, limit], sqlite_communication_row)? {
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

pub fn export_communication_actions(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<JobsCommunicationActionExport>> {
    let actions: Vec<JobsCommunicationAction> = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(&format!(
                "SELECT {COMMUNICATION_ACTION_SELECT} FROM jobs_communication_actions
                  WHERE account_id = ?1 ORDER BY created_at_ms, id"
            ))?;
            let rows = stmt.query_map(params![account_id], sqlite_communication_row)?;
            rows.map(|row| communication_action_from_row(row?))
                .collect::<Result<Vec<_>>>()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT} FROM jobs_communication_actions
                      WHERE account_id = $1 ORDER BY created_at_ms, id"
                ),
                &[&account_id],
            )?
            .into_iter()
            .map(postgres_communication_row)
            .map(communication_action_from_row)
            .collect(),
    })?;
    Ok(actions
        .into_iter()
        .map(|action| JobsCommunicationActionExport {
            id: action.id,
            application_id: action.application_id,
            connection_id: action.connection_id,
            source_message_id: action.source_message_id,
            kind: action.kind,
            provider: action.provider,
            payload: action.payload,
            status: action.status,
            action_revision: action.action_revision,
            approval_revision: action.approval_revision,
            approved_at_ms: action.approved_at_ms,
            dispatched_at_ms: action.dispatched_at_ms,
            created_at_ms: action.created_at_ms,
            updated_at_ms: action.updated_at_ms,
        })
        .collect())
}

pub fn export_communication_evidence(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<JobsCommunicationEvidenceExport>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT evidence.action_id, evidence.event_kind, evidence.recorded_at_ms
                   FROM jobs_communication_action_attempt_evidence evidence
                  WHERE evidence.account_id = ?1
                  ORDER BY evidence.recorded_at_ms, evidence.id",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            rows.map(|row| {
                let (action_id, event_kind, recorded) = row?;
                Ok(JobsCommunicationEvidenceExport {
                    action_id,
                    event_kind,
                    recorded_at_ms: recorded,
                })
            })
            .collect::<Result<Vec<_>>>()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT evidence.action_id, evidence.event_kind, evidence.recorded_at_ms
                   FROM jobs_communication_action_attempt_evidence evidence
                  WHERE evidence.account_id = $1
                  ORDER BY evidence.recorded_at_ms, evidence.id",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                Ok(JobsCommunicationEvidenceExport {
                    action_id: row.get(0),
                    event_kind: row.get(1),
                    recorded_at_ms: row.get(2),
                })
            })
            .collect(),
    })
}

pub fn export_communication_reconciliations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<JobsCommunicationReconciliationExport>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT reconciliation.action_id, reconciliation.resolution,
                        reconciliation.recorded_at_ms
                   FROM jobs_communication_action_reconciliations reconciliation
                  WHERE reconciliation.account_id = ?1
                  ORDER BY reconciliation.recorded_at_ms, reconciliation.id",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            rows.map(|row| {
                let (action_id, resolution, recorded) = row?;
                Ok(JobsCommunicationReconciliationExport {
                    action_id,
                    resolution,
                    recorded_at_ms: recorded,
                })
            })
            .collect::<Result<Vec<_>>>()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT reconciliation.action_id, reconciliation.resolution,
                        reconciliation.recorded_at_ms
                   FROM jobs_communication_action_reconciliations reconciliation
                  WHERE reconciliation.account_id = $1
                  ORDER BY reconciliation.recorded_at_ms, reconciliation.id",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                Ok(JobsCommunicationReconciliationExport {
                    action_id: row.get(0),
                    resolution: row.get(1),
                    recorded_at_ms: row.get(2),
                })
            })
            .collect(),
    })
}

/// Returns server-authoritative launch readiness for the review UI. This never
/// exposes provider credentials: callers receive only a boolean and a stable,
/// user-actionable reason when execution is unavailable.
pub fn communication_action_execution_readiness(
    pool: &DbPool,
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<(bool, String)> {
    let dispatch_enabled = communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED");
    if !dispatch_enabled {
        return Ok((
            false,
            "Communication execution is not enabled on this server.".to_string(),
        ));
    }
    if communication_write_is_fenced(pool, account_id, &action.connection_id)? {
        return Ok((
            false,
            "Communication writes are draining for account or mailbox deletion.".to_string(),
        ));
    }
    if !matches!(
        action.status.as_str(),
        "awaiting_approval" | "needs_input" | "approved"
    ) {
        return Ok((
            false,
            "This communication is not awaiting executable approval.".to_string(),
        ));
    }
    if action.action_revision
        > COMMUNICATION_ACTION_REVISION_MAX - COMMUNICATION_ACTION_REVISION_LIFECYCLE_HEADROOM
        || action.updated_at_ms == i64::MAX
    {
        return Ok((
            false,
            "This draft exhausted its portable revision authority; cancel it and review a fresh draft."
                .to_string(),
        ));
    }
    if action.attempt_count >= COMMUNICATION_ACTION_MAX_ATTEMPTS {
        return Ok((
            false,
            "This draft reached its bounded provider-attempt limit; cancel it and review a fresh draft."
                .to_string(),
        ));
    }
    let authority_is_current = communication_authority_sha256(account_id, action)
        .is_ok_and(|digest| digest == action.authority_sha256);
    if validate_communication_kind_provider(&action.kind, &action.provider).is_err()
        || validate_communication_payload(&action.kind, &action.payload).is_err()
        || !authority_is_current
    {
        return Ok((
            false,
            "This communication draft no longer matches its approved authority.".to_string(),
        ));
    }
    if validate_communication_relationships(pool, account_id, action).is_err() {
        return Ok((
            false,
            "The application, mailbox, or source message is no longer available.".to_string(),
        ));
    }
    let Some(mailbox) = mailbox_connection(pool, account_id, &action.connection_id)? else {
        return Ok((
            false,
            "Reconnect this mailbox before approving communication.".to_string(),
        ));
    };
    let Some(credential) = jobs_provider_credential(pool, account_id, &action.connection_id)?
    else {
        return Ok((
            false,
            "Authorize the required provider write access before approving communication."
                .to_string(),
        ));
    };
    let Ok((grant_revision, grant_sha256)) =
        validate_communication_grant(&action.provider, &mailbox, &credential)
    else {
        return Ok((
            false,
            "Reconnect this mailbox and authorize the required provider write access.".to_string(),
        ));
    };
    if action.status == "approved"
        && (action.approval_revision <= 0
            || action.approved_authority_sha256 != action.authority_sha256
            || action.approved_grant_revision != grant_revision
            || action.approved_grant_sha256 != grant_sha256)
    {
        return Ok((
            false,
            "The provider grant changed; cancel this approved draft, reconnect the mailbox, and review a fresh draft."
                .to_string(),
        ));
    }
    Ok((true, String::new()))
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
        let message = crate::db::run_blocking_db(|| -> Result<Option<JobsProviderMessage>> {
            match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    let raw: Option<String> = conn
                        .query_row(
                            "SELECT message_json FROM jobs_provider_messages
                          WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                            AND application_id = ?4",
                            params![
                                account_id,
                                action.connection_id,
                                message_id,
                                action.application_id,
                            ],
                            |row| row.get(0),
                        )
                        .optional()?;
                    raw.map(|value| parse_json(value, "Jobs provider message"))
                        .transpose()
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    conn.query_opt(
                        "SELECT message_json FROM jobs_provider_messages
                          WHERE account_id = $1 AND connection_id = $2 AND id = $3
                            AND application_id = $4",
                        &[
                            &account_id,
                            &action.connection_id,
                            &message_id,
                            &action.application_id,
                        ],
                    )?
                    .map(|row| parse_json(row.get(0), "Jobs provider message"))
                    .transpose()
                }
            }
        })?;
        let message = message.ok_or_else(|| anyhow::anyhow!("source mailbox message not found"))?;
        validate_communication_reply_source(action, &message)?;
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
    validate_communication_kind_provider(&value.kind, &value.provider)?;
    value.source_message_id = value
        .source_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if value.kind == "reply" && value.source_message_id.is_none() {
        anyhow::bail!("communication replies require a source mailbox message")
    }
    if value.kind == "calendar" && value.source_message_id.is_some() {
        anyhow::bail!("calendar actions cannot bind a source mailbox message")
    }
    value.idempotency_key = value.idempotency_key.trim().to_string();
    if value.application_id.trim().is_empty()
        || value.connection_id.trim().is_empty()
        || value.idempotency_key.is_empty()
        || value.idempotency_key.len() > 240
        || value
            .idempotency_key
            .bytes()
            .any(|byte| byte.is_ascii_control())
    {
        anyhow::bail!("communication action is incomplete")
    }
    normalize_communication_payload_emails(&value.kind, &mut value.payload)?;
    value.payload = canonical_communication_value(&value.payload);
    validate_communication_payload(&value.kind, &value.payload)?;
    value.payload_sha256 = communication_payload_sha256(&value.payload)?;
    value.authority_sha256 = communication_authority_sha256(account_id, &value)?;
    value.status = "awaiting_approval".to_string();
    value.provider_object_id.clear();
    value.lease_owner = None;
    value.lease_kind = None;
    value.lease_expires_at_ms = None;
    value.active_attempt_id = None;
    value.attempt_count = 0;
    value.reconciliation_count = 0;
    value.action_revision = 1;
    value.approval_revision = 0;
    value.approved_authority_sha256.clear();
    value.approved_grant_revision = 0;
    value.approved_grant_sha256.clear();
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
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            require_communication_write_unfenced_sqlite_tx(&tx, account_id, &value.connection_id)?;
            validate_communication_kind_provider(&value.kind, &value.provider)?;
            validate_communication_payload(&value.kind, &value.payload)?;
            if communication_authority_sha256(account_id, &value)? != value.authority_sha256 {
                anyhow::bail!("communication action authority changed before creation")
            }
            tx.query_row(
                "SELECT 1 FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                params![account_id, value.application_id],
                |_| Ok(()),
            )?;
            tx.query_row(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = ?1 AND id = ?2 AND status = 'connected'
                    AND provider = ?3",
                params![
                    account_id,
                    value.connection_id,
                    communication_connection_provider(&value.provider),
                ],
                |_| Ok(()),
            )?;
            if value.kind == "reply" {
                let source_id = value
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_json: String = tx.query_row(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                        AND application_id = ?4",
                    params![
                        account_id,
                        value.connection_id,
                        source_id,
                        value.application_id
                    ],
                    |row| row.get(0),
                )?;
                let source: JobsProviderMessage = parse_json(source_json, "Jobs provider message")?;
                validate_communication_reply_source(&value, &source)?;
            }
            let inserted = tx.execute(
                "INSERT INTO jobs_communication_actions (
                    id, account_id, application_id, connection_id, source_message_id,
                    kind, provider, idempotency_key, payload_sha256, authority_sha256,
                    status, action_json,
                    next_attempt_at_ms, created_at_ms, updated_at_ms, action_revision
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1)
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
                    value.authority_sha256,
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
                || stored.source_message_id != value.source_message_id
                || stored.authority_sha256 != value.authority_sha256
            {
                anyhow::bail!("communication action idempotency key was reused")
            }
            Ok((stored, inserted))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            require_communication_write_unfenced_postgres_tx(
                &mut tx,
                account_id,
                &value.connection_id,
            )?;
            validate_communication_kind_provider(&value.kind, &value.provider)?;
            validate_communication_payload(&value.kind, &value.payload)?;
            if communication_authority_sha256(account_id, &value)? != value.authority_sha256 {
                anyhow::bail!("communication action authority changed before creation")
            }
            tx.query_one(
                "SELECT 1 FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR SHARE",
                &[&account_id, &value.application_id],
            )?;
            tx.query_one(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2 AND status = 'connected'
                    AND provider = $3 FOR SHARE",
                &[
                    &account_id,
                    &value.connection_id,
                    &communication_connection_provider(&value.provider),
                ],
            )?;
            if value.kind == "reply" {
                let source_id = value
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_row = tx.query_one(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = $1 AND connection_id = $2 AND id = $3
                        AND application_id = $4 FOR SHARE",
                    &[
                        &account_id,
                        &value.connection_id,
                        &source_id,
                        &value.application_id,
                    ],
                )?;
                let source: JobsProviderMessage =
                    parse_json(source_row.get(0), "Jobs provider message")?;
                validate_communication_reply_source(&value, &source)?;
            }
            let inserted = tx.execute(
                "INSERT INTO jobs_communication_actions (
                    id, account_id, application_id, connection_id, source_message_id,
                    kind, provider, idempotency_key, payload_sha256, authority_sha256,
                    status, action_json,
                    next_attempt_at_ms, created_at_ms, updated_at_ms, action_revision
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 1)
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
                    &value.authority_sha256,
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
                || stored.source_message_id != value.source_message_id
                || stored.authority_sha256 != value.authority_sha256
            {
                anyhow::bail!("communication action idempotency key was reused")
            }
            Ok((stored, inserted))
        }
    })
}

pub fn approve_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
    expected_action_revision: i64,
    expected_payload_sha256: &str,
) -> Result<Option<JobsCommunicationAction>> {
    if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
        anyhow::bail!("communication execution is not enabled")
    }
    let Some(action) = communication_action(pool, account_id, action_id)? else {
        return Ok(None);
    };
    if !(1..=COMMUNICATION_ACTION_REVISION_MAX).contains(&expected_action_revision)
        || !exact_lowercase_sha256(expected_payload_sha256)
        || action.action_revision != expected_action_revision
        || action.payload_sha256 != expected_payload_sha256
    {
        anyhow::bail!("communication action review snapshot changed")
    }
    if !matches!(action.status.as_str(), "awaiting_approval" | "needs_input")
        || action.attempt_count >= COMMUNICATION_ACTION_MAX_ATTEMPTS
    {
        anyhow::bail!("communication action cannot be approved in its current state")
    }
    if action.action_revision
        > COMMUNICATION_ACTION_REVISION_MAX - COMMUNICATION_ACTION_REVISION_LIFECYCLE_HEADROOM
        || action.updated_at_ms == i64::MAX
    {
        anyhow::bail!("communication action revision authority is exhausted")
    }
    let authority_sha256 = communication_authority_sha256(account_id, &action)?;
    if action.authority_sha256 != authority_sha256 {
        anyhow::bail!("communication action authority digest is invalid")
    }
    validate_communication_relationships(pool, account_id, &action)?;
    let now = now_ms();

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            let current_row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            let current = communication_action_from_row(current_row)?;
            require_communication_write_unfenced_sqlite_tx(
                &tx,
                account_id,
                &current.connection_id,
            )?;
            if current.action_revision != expected_action_revision
                || current.payload_sha256 != expected_payload_sha256
            {
                anyhow::bail!("communication action review snapshot changed")
            }
            if !matches!(current.status.as_str(), "awaiting_approval" | "needs_input")
                || current.attempt_count >= COMMUNICATION_ACTION_MAX_ATTEMPTS
            {
                anyhow::bail!("communication action cannot be approved in its current state")
            }
            if current.action_revision
                > COMMUNICATION_ACTION_REVISION_MAX
                    - COMMUNICATION_ACTION_REVISION_LIFECYCLE_HEADROOM
                || current.updated_at_ms == i64::MAX
            {
                anyhow::bail!("communication action revision authority is exhausted")
            }
            validate_communication_kind_provider(&current.kind, &current.provider)?;
            validate_communication_payload(&current.kind, &current.payload)?;
            let current_authority = communication_authority_sha256(account_id, &current)?;
            if current_authority != authority_sha256
                || current.authority_sha256 != current_authority
            {
                anyhow::bail!("communication action authority changed before approval")
            }
            tx.query_row(
                "SELECT 1 FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                params![account_id, current.application_id],
                |_| Ok(()),
            )?;
            if current.kind == "reply" {
                let source_id = current
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_json: String = tx.query_row(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                        AND application_id = ?4",
                    params![
                        account_id,
                        current.connection_id,
                        source_id,
                        current.application_id,
                    ],
                    |row| row.get(0),
                )?;
                let source: JobsProviderMessage = parse_json(source_json, "Jobs provider message")?;
                validate_communication_reply_source(&current, &source)?;
            }
            let (mailbox_json, credential_json): (String, String) = tx.query_row(
                "SELECT mailbox.connection_json, credential.credential_json
                   FROM jobs_mailbox_connections mailbox
                   JOIN jobs_provider_credentials credential
                     ON credential.account_id = mailbox.account_id
                    AND credential.connection_id = mailbox.id
                  WHERE mailbox.account_id = ?1 AND mailbox.id = ?2
                    AND mailbox.status = 'connected'",
                params![account_id, current.connection_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let mailbox: MailboxConnection = parse_json(mailbox_json, "mailbox connection")?;
            let credential: JobsProviderCredential =
                parse_json(credential_json, "Jobs provider credential")?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&current.provider, &mailbox, &credential)?;
            let effective_timestamp = current
                .updated_at_ms
                .checked_add(1)
                .map(|next| next.max(now))
                .ok_or_else(|| anyhow::anyhow!("communication action timestamp overflow"))?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'approved', approval_revision = approval_revision + 1,
                        action_revision = action_revision + 1,
                        approved_authority_sha256 = ?3, approved_grant_revision = ?4,
                        approved_grant_sha256 = ?5, approved_at_ms = ?6,
                        next_attempt_at_ms = ?9,
                        updated_at_ms = MAX(?6, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?1 AND id = ?2
                    AND status IN ('awaiting_approval', 'needs_input')
                    AND authority_sha256 = ?3
                    AND action_revision = ?7 AND payload_sha256 = ?8
                    AND action_revision <= 9007199254740943
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![
                    account_id,
                    action_id,
                    authority_sha256,
                    grant_revision,
                    grant_sha256,
                    effective_timestamp,
                    expected_action_revision,
                    expected_payload_sha256,
                    now,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("communication action cannot be approved in its current state")
            }
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            let approved = communication_action_from_row(row)?;
            if approved.status != "approved"
                || approved.approved_authority_sha256 != authority_sha256
                || approved.approved_grant_revision != grant_revision
                || approved.approved_grant_sha256 != grant_sha256
            {
                anyhow::bail!("communication action approval binding changed")
            }
            tx.commit()?;
            Ok(Some(approved))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            let current_row = tx.query_one(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE"
                ),
                &[&account_id, &action_id],
            )?;
            let current = communication_action_from_row(postgres_communication_row(current_row))?;
            require_communication_write_unfenced_postgres_tx(
                &mut tx,
                account_id,
                &current.connection_id,
            )?;
            if current.action_revision != expected_action_revision
                || current.payload_sha256 != expected_payload_sha256
            {
                anyhow::bail!("communication action review snapshot changed")
            }
            if !matches!(current.status.as_str(), "awaiting_approval" | "needs_input")
                || current.attempt_count >= COMMUNICATION_ACTION_MAX_ATTEMPTS
            {
                anyhow::bail!("communication action cannot be approved in its current state")
            }
            if current.action_revision
                > COMMUNICATION_ACTION_REVISION_MAX
                    - COMMUNICATION_ACTION_REVISION_LIFECYCLE_HEADROOM
                || current.updated_at_ms == i64::MAX
            {
                anyhow::bail!("communication action revision authority is exhausted")
            }
            validate_communication_kind_provider(&current.kind, &current.provider)?;
            validate_communication_payload(&current.kind, &current.payload)?;
            let current_authority = communication_authority_sha256(account_id, &current)?;
            if current_authority != authority_sha256
                || current.authority_sha256 != current_authority
            {
                anyhow::bail!("communication action authority changed before approval")
            }
            tx.query_one(
                "SELECT 1 FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR SHARE",
                &[&account_id, &current.application_id],
            )?;
            if current.kind == "reply" {
                let source_id = current
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_row = tx.query_one(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = $1 AND connection_id = $2 AND id = $3
                        AND application_id = $4 FOR SHARE",
                    &[
                        &account_id,
                        &current.connection_id,
                        &source_id,
                        &current.application_id,
                    ],
                )?;
                let source: JobsProviderMessage =
                    parse_json(source_row.get(0), "Jobs provider message")?;
                validate_communication_reply_source(&current, &source)?;
            }
            let grant_row = tx.query_one(
                "SELECT mailbox.connection_json, credential.credential_json
                   FROM jobs_mailbox_connections mailbox
                   JOIN jobs_provider_credentials credential
                     ON credential.account_id = mailbox.account_id
                    AND credential.connection_id = mailbox.id
                  WHERE mailbox.account_id = $1 AND mailbox.id = $2
                    AND mailbox.status = 'connected'
                  FOR UPDATE OF mailbox, credential",
                &[&account_id, &current.connection_id],
            )?;
            let mailbox: MailboxConnection = parse_json(grant_row.get(0), "mailbox connection")?;
            let credential: JobsProviderCredential =
                parse_json(grant_row.get(1), "Jobs provider credential")?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&current.provider, &mailbox, &credential)?;
            let effective_timestamp = current
                .updated_at_ms
                .checked_add(1)
                .map(|next| next.max(now))
                .ok_or_else(|| anyhow::anyhow!("communication action timestamp overflow"))?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'approved', approval_revision = approval_revision + 1,
                        action_revision = action_revision + 1,
                        approved_authority_sha256 = $3, approved_grant_revision = $4,
                        approved_grant_sha256 = $5, approved_at_ms = $6,
                        next_attempt_at_ms = $9,
                        updated_at_ms = GREATEST($6, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = $1 AND id = $2
                    AND status IN ('awaiting_approval', 'needs_input')
                    AND authority_sha256 = $3
                    AND action_revision = $7 AND payload_sha256 = $8
                    AND action_revision <= 9007199254740943
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                &[
                    &account_id,
                    &action_id,
                    &authority_sha256,
                    &grant_revision,
                    &grant_sha256,
                    &effective_timestamp,
                    &expected_action_revision,
                    &expected_payload_sha256,
                    &now,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("communication action cannot be approved in its current state")
            }
            let row = tx.query_one(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &action_id],
            )?;
            let approved = communication_action_from_row(postgres_communication_row(row))?;
            if approved.status != "approved"
                || approved.approved_authority_sha256 != authority_sha256
                || approved.approved_grant_revision != grant_revision
                || approved.approved_grant_sha256 != grant_sha256
            {
                anyhow::bail!("communication action approval binding changed")
            }
            tx.commit()?;
            Ok(Some(approved))
        }
    })
}

pub fn cancel_communication_action(
    pool: &DbPool,
    account_id: &str,
    action_id: &str,
    expected_action_revision: i64,
    expected_payload_sha256: &str,
) -> Result<Option<JobsCommunicationAction>> {
    if !(1..=COMMUNICATION_ACTION_REVISION_MAX).contains(&expected_action_revision)
        || !exact_lowercase_sha256(expected_payload_sha256)
    {
        anyhow::bail!("communication action review snapshot is invalid")
    }
    let now = now_ms();
    let action = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'cancelled', lease_owner = NULL, lease_kind = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        active_attempt_id = NULL, action_revision = action_revision + 1,
                        updated_at_ms = MAX(?3, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?1 AND id = ?2
                    AND status IN ('awaiting_approval', 'needs_input', 'approved')
                    AND action_revision = ?4 AND payload_sha256 = ?5
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![
                    account_id,
                    action_id,
                    now,
                    expected_action_revision,
                    expected_payload_sha256,
                ],
            )?;
            let row = tx
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
            let action = row.map(communication_action_from_row).transpose()?;
            if action.is_some() && changed != 1 {
                anyhow::bail!("communication action review snapshot changed")
            }
            tx.commit()?;
            Ok(action)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'cancelled', lease_owner = NULL, lease_kind = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        active_attempt_id = NULL, action_revision = action_revision + 1,
                        updated_at_ms = GREATEST($3, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = $1 AND id = $2
                    AND status IN ('awaiting_approval', 'needs_input', 'approved')
                    AND action_revision = $4 AND payload_sha256 = $5
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                &[
                    &account_id,
                    &action_id,
                    &now,
                    &expected_action_revision,
                    &expected_payload_sha256,
                ],
            )?;
            let row = tx.query_opt(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &action_id],
            )?;
            let action = row
                .map(postgres_communication_row)
                .map(communication_action_from_row)
                .transpose()?;
            if action.is_some() && changed != 1 {
                anyhow::bail!("communication action review snapshot changed")
            }
            tx.commit()?;
            Ok(action)
        }
    })?;
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
    if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
        anyhow::bail!("communication execution is not enabled")
    }
    if owner_id.trim().is_empty() || owner_id.len() > 240 {
        anyhow::bail!("communication worker identity is invalid")
    }
    let now = now_ms();
    let expires_at = now.saturating_add(COMMUNICATION_ACTION_LEASE_MS);
    let lease_token = random_execution_lease_token();
    let token_hash = execution_lease_token_hash(&lease_token);
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let provider_operation_key = format!("bluey-{}", uuid::Uuid::new_v4());
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            let expired: Option<(String, String)> = tx
                .query_row(
                    "SELECT id, account_id FROM jobs_communication_actions
                      WHERE status = 'dispatching' AND lease_expires_at_ms <= ?1
                      ORDER BY lease_expires_at_ms, id LIMIT 1",
                    params![now],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((expired_action_id, expired_account_id)) = expired {
                crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                    &tx,
                    &expired_account_id,
                )?;
                let changed = tx.execute(
                    "UPDATE jobs_communication_actions
                        SET status = 'side_effect_unknown',
                            action_revision = action_revision + 1,
                            updated_at_ms = MAX(?1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = ?2 AND id = ?3 AND status = 'dispatching'
                        AND lease_expires_at_ms <= ?1
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807",
                    params![now, expired_account_id, expired_action_id],
                )?;
                if changed != 1 {
                    anyhow::bail!("expired communication dispatch authority changed")
                }
                tx.commit()?;
                return Ok(None);
            }
            type DispatchCandidate = (
                String,
                String,
                i64,
                String,
                String,
                String,
                String,
                i64,
                i64,
            );
            let initial_scan_cursor =
                operational_hold_scan_cursor_snapshot(&COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS);
            let mut scan_cursor = initial_scan_cursor.clone();
            let mut wrapped = false;
            let mut scanned = 0;
            let mut exhausted = false;
            let selected = loop {
                if scanned >= OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT {
                    break None;
                }
                let cursor_next_attempt = scan_cursor.as_ref().map(|value| value.0);
                let cursor_created = scan_cursor.as_ref().map(|value| value.1);
                let cursor_id = scan_cursor
                    .as_ref()
                    .map(|value| value.2.as_str())
                    .unwrap_or_default();
                let candidate: Option<DispatchCandidate> = tx
                    .query_row(
                        "SELECT a.id, a.account_id, a.fence, c.connection_json,
                                credential.credential_json, a.application_id, c.provider,
                                a.next_attempt_at_ms, a.created_at_ms
                           FROM jobs_communication_actions a
                           JOIN jobs_mailbox_connections c
                             ON c.id = a.connection_id
                            AND c.account_id = a.account_id
                            AND c.status = 'connected'
                           JOIN jobs_provider_credentials credential
                             ON credential.connection_id = a.connection_id
                            AND credential.account_id = a.account_id
                          WHERE a.status = 'approved'
                            AND a.next_attempt_at_ms <= ?1 AND a.attempt_count < ?2
                            AND a.action_revision <= 9007199254740943
                            AND a.approval_revision > 0
                            AND length(a.authority_sha256) = 64
                            AND a.approved_authority_sha256 = a.authority_sha256
                            AND (?3 IS NULL OR a.next_attempt_at_ms > ?3
                              OR (a.next_attempt_at_ms = ?3 AND a.created_at_ms > ?4)
                              OR (a.next_attempt_at_ms = ?3 AND a.created_at_ms = ?4
                                AND a.id > ?5))
                            AND NOT EXISTS (
                              SELECT 1 FROM account_deletion_intents deletion
                               WHERE deletion.account_id = a.account_id
                            )
                            AND NOT EXISTS (
                              SELECT 1 FROM jobs_communication_write_fences fence
                               WHERE fence.account_id = a.account_id
                                 AND fence.connection_id IN ('', a.connection_id)
                            )
                            AND (
                              a.kind <> 'reply' OR EXISTS (
                                SELECT 1 FROM jobs_provider_messages message
                                 WHERE message.id = a.source_message_id
                                   AND message.account_id = a.account_id
                                   AND message.connection_id = a.connection_id
                                   AND message.application_id = a.application_id
                              )
                            )
                            AND (
                              (a.provider IN ('gmail', 'google_calendar') AND c.provider = 'gmail')
                              OR
                              (a.provider IN ('outlook_email', 'outlook_calendar')
                                AND c.provider = 'outlook')
                            )
                          ORDER BY a.next_attempt_at_ms ASC, a.created_at_ms ASC, a.id ASC
                          LIMIT 1",
                        params![
                            now,
                            COMMUNICATION_ACTION_MAX_ATTEMPTS,
                            cursor_next_attempt,
                            cursor_created,
                            cursor_id,
                        ],
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
                let Some(candidate) = candidate else {
                    if !wrapped && initial_scan_cursor.is_some() {
                        scan_cursor = None;
                        wrapped = true;
                        continue;
                    }
                    exhausted = true;
                    break None;
                };
                let candidate_cursor = (candidate.7, candidate.8, candidate.0.clone());
                if wrapped
                    && initial_scan_cursor
                        .as_ref()
                        .is_some_and(|initial| &candidate_cursor >= initial)
                {
                    exhausted = true;
                    break None;
                }
                scan_cursor = Some(candidate_cursor);
                scanned += 1;
                let mailbox: MailboxConnection = parse_json(candidate.3, "mailbox connection")?;
                if mailbox.provider != candidate.6 {
                    anyhow::bail!("communication mailbox authority changed")
                }
                if communication_dispatch_is_held_sqlite_tx(
                    &tx,
                    &candidate.1,
                    &candidate.5,
                    &candidate.6,
                )? {
                    continue;
                }
                break Some((candidate.0, candidate.1, candidate.2, mailbox, candidate.4));
            };
            let next_scan_cursor = (selected.is_none()
                && !exhausted
                && scanned == OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT)
                .then(|| scan_cursor.clone())
                .flatten();
            let Some((action_id, account_id, fence, mailbox, credential_json)) = selected else {
                tx.commit()?;
                compare_exchange_operational_hold_scan_cursor(
                    &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                    initial_scan_cursor.as_ref(),
                    next_scan_cursor,
                );
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
            let fence = fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            let credential: JobsProviderCredential =
                parse_json(credential_json, "Jobs provider credential")?;
            let current = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            let current = communication_action_from_row(current)?;
            require_communication_write_unfenced_sqlite_tx(
                &tx,
                &account_id,
                &current.connection_id,
            )?;
            validate_communication_kind_provider(&current.kind, &current.provider)?;
            validate_communication_payload(&current.kind, &current.payload)?;
            if current.kind == "reply" {
                let source_message_id = current
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_json: String = tx.query_row(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                        AND application_id = ?4",
                    params![
                        account_id,
                        current.connection_id,
                        source_message_id,
                        current.application_id,
                    ],
                    |row| row.get(0),
                )?;
                let source: JobsProviderMessage = parse_json(source_json, "Jobs provider message")?;
                validate_communication_reply_source(&current, &source)?;
            }
            let authority_sha256 = communication_authority_sha256(&account_id, &current)?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&current.provider, &mailbox, &credential)?;
            if current.authority_sha256 != authority_sha256
                || current.approved_authority_sha256 != authority_sha256
                || current.approved_grant_revision != grant_revision
                || current.approved_grant_sha256 != grant_sha256
            {
                anyhow::bail!("communication action approval or provider grant changed")
            }
            tx.execute(
                "INSERT INTO jobs_communication_action_attempts (
                    id, account_id, action_id, connection_id, provider, dispatch_no, fence,
                    approval_revision, authority_sha256, grant_revision, grant_sha256,
                    provider_operation_key, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    attempt_id,
                    account_id,
                    action_id,
                    current.connection_id,
                    current.provider,
                    current.attempt_count + 1,
                    fence,
                    current.approval_revision,
                    authority_sha256,
                    grant_revision,
                    grant_sha256,
                    provider_operation_key,
                    now,
                ],
            )?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'dispatching', lease_owner = ?2, lease_kind = 'dispatch',
                        lease_token_sha256 = ?3, fence = ?4, lease_expires_at_ms = ?5,
                        active_attempt_id = ?6, attempt_count = attempt_count + 1,
                        action_revision = action_revision + 1,
                        dispatched_at_ms = COALESCE(dispatched_at_ms, ?1),
                        updated_at_ms = MAX(?1, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE id = ?7 AND account_id = ?8 AND status = 'approved'
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![
                    now, owner_id, token_hash, fence, expires_at, attempt_id, action_id,
                    account_id,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("communication action approval changed before dispatch")
            }
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, action_id],
                sqlite_communication_row,
            )?;
            tx.commit()?;
            compare_exchange_operational_hold_scan_cursor(
                &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                initial_scan_cursor.as_ref(),
                next_scan_cursor,
            );
            Ok(Some(JobsCommunicationActionLease {
                account_id,
                action: communication_action_from_row(row)?,
                attempt_id,
                lease_kind: "dispatch".to_string(),
                provider_operation_key,
                authority_sha256,
                approval_revision: current.approval_revision,
                grant_revision,
                grant_sha256,
                lease_token,
                fence,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx)?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            if let Some(expired) = tx.query_opt(
                "SELECT id, account_id FROM jobs_communication_actions
                  WHERE status = 'dispatching' AND lease_expires_at_ms <= $1
                  ORDER BY lease_expires_at_ms, id LIMIT 1",
                &[&now],
            )? {
                let expired_action_id: String = expired.get(0);
                let expired_account_id: String = expired.get(1);
                lock_discovery_account_shared_postgres(&mut tx, &expired_account_id)?;
                crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                    &mut tx,
                    &expired_account_id,
                )?;
                let locked_expiry = tx
                    .query_opt(
                        "SELECT lease_expires_at_ms FROM jobs_communication_actions
                          WHERE account_id = $1 AND id = $2 AND status = 'dispatching'
                          FOR UPDATE",
                        &[&expired_account_id, &expired_action_id],
                    )?
                    .map(|row| row.get::<_, Option<i64>>(0));
                let Some(Some(locked_expiry)) = locked_expiry else {
                    tx.commit()?;
                    return Ok(None);
                };
                let reclaim_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
                if locked_expiry > reclaim_now_ms {
                    tx.commit()?;
                    return Ok(None);
                }
                let changed = tx.execute(
                    "UPDATE jobs_communication_actions
                        SET status = 'side_effect_unknown',
                            action_revision = action_revision + 1,
                            updated_at_ms = GREATEST($1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = $2 AND id = $3 AND status = 'dispatching'
                        AND lease_expires_at_ms <= $1
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807",
                    &[&reclaim_now_ms, &expired_account_id, &expired_action_id],
                )?;
                if changed != 1 {
                    anyhow::bail!("expired communication dispatch authority changed")
                }
                tx.commit()?;
                return Ok(None);
            }
            let initial_scan_cursor =
                operational_hold_scan_cursor_snapshot(&COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS);
            let mut scan_cursor = initial_scan_cursor.clone();
            let mut wrapped = false;
            let mut scanned = 0;
            let mut exhausted = false;
            let selected = loop {
                if scanned >= OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT {
                    break None;
                }
                let cursor_next_attempt = scan_cursor.as_ref().map(|value| value.0);
                let cursor_created = scan_cursor.as_ref().map(|value| value.1);
                let cursor_id = scan_cursor
                    .as_ref()
                    .map(|value| value.2.as_str())
                    .unwrap_or_default();
                let candidate = tx.query_opt(
                    "SELECT a.id, a.account_id, a.application_id, c.provider,
                            a.next_attempt_at_ms, a.created_at_ms
                       FROM jobs_communication_actions a
                       JOIN jobs_mailbox_connections c
                         ON c.id = a.connection_id
                        AND c.account_id = a.account_id
                        AND c.status = 'connected'
                       JOIN jobs_provider_credentials credential
                         ON credential.connection_id = a.connection_id
                        AND credential.account_id = a.account_id
                      WHERE a.status = 'approved'
                        AND a.next_attempt_at_ms <= $1 AND a.attempt_count < $2
                        AND a.action_revision <= 9007199254740943
                        AND a.approval_revision > 0
                        AND length(a.authority_sha256) = 64
                        AND a.approved_authority_sha256 = a.authority_sha256
                        AND ($3::bigint IS NULL OR a.next_attempt_at_ms > $3
                          OR (a.next_attempt_at_ms = $3 AND a.created_at_ms > $4)
                          OR (a.next_attempt_at_ms = $3 AND a.created_at_ms = $4
                            AND a.id > $5))
                        AND NOT EXISTS (
                          SELECT 1 FROM account_deletion_intents deletion
                           WHERE deletion.account_id = a.account_id
                        )
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_communication_write_fences fence
                           WHERE fence.account_id = a.account_id
                             AND fence.connection_id IN ('', a.connection_id)
                        )
                        AND (
                          a.kind <> 'reply' OR EXISTS (
                            SELECT 1 FROM jobs_provider_messages message
                             WHERE message.id = a.source_message_id
                               AND message.account_id = a.account_id
                               AND message.connection_id = a.connection_id
                               AND message.application_id = a.application_id
                          )
                        )
                        AND (
                          (a.provider IN ('gmail', 'google_calendar') AND c.provider = 'gmail')
                          OR
                          (a.provider IN ('outlook_email', 'outlook_calendar')
                            AND c.provider = 'outlook')
                        )
                      ORDER BY a.next_attempt_at_ms ASC, a.created_at_ms ASC, a.id ASC
                      LIMIT 1",
                    &[
                        &now,
                        &COMMUNICATION_ACTION_MAX_ATTEMPTS,
                        &cursor_next_attempt,
                        &cursor_created,
                        &cursor_id,
                    ],
                )?;
                let Some(candidate) = candidate else {
                    if !wrapped && initial_scan_cursor.is_some() {
                        scan_cursor = None;
                        wrapped = true;
                        continue;
                    }
                    exhausted = true;
                    break None;
                };
                let candidate_account_id = candidate.get::<_, String>(1);
                let application_id = candidate.get::<_, String>(2);
                let mailbox_provider = candidate.get::<_, String>(3);
                let candidate_cursor = (
                    candidate.get::<_, i64>(4),
                    candidate.get::<_, i64>(5),
                    candidate.get::<_, String>(0),
                );
                if wrapped
                    && initial_scan_cursor
                        .as_ref()
                        .is_some_and(|initial| &candidate_cursor >= initial)
                {
                    exhausted = true;
                    break None;
                }
                scan_cursor = Some(candidate_cursor.clone());
                scanned += 1;
                lock_discovery_account_shared_postgres(&mut tx, &candidate_account_id)?;
                if communication_dispatch_is_held_postgres_tx_after_authority_prelock(
                    &mut tx,
                    &candidate_account_id,
                    &application_id,
                    &mailbox_provider,
                )? {
                    continue;
                }
                break Some((
                    candidate_cursor.2,
                    candidate_account_id,
                    application_id,
                    mailbox_provider,
                ));
            };
            let next_scan_cursor = (selected.is_none()
                && !exhausted
                && scanned == OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT)
                .then(|| scan_cursor.clone())
                .flatten();
            let Some((action_id, account_id, application_id, mailbox_provider)) = selected else {
                tx.commit()?;
                compare_exchange_operational_hold_scan_cursor(
                    &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                    initial_scan_cursor.as_ref(),
                    next_scan_cursor,
                );
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let locked = tx.query_opt(
                "SELECT a.fence, c.connection_json, credential.credential_json, c.provider
                   FROM jobs_communication_actions a
                   JOIN jobs_mailbox_connections c
                     ON c.id = a.connection_id
                    AND c.account_id = a.account_id
                    AND c.status = 'connected'
                   JOIN jobs_provider_credentials credential
                     ON credential.connection_id = a.connection_id
                    AND credential.account_id = a.account_id
                  WHERE a.id = $1 AND a.account_id = $2 AND a.status = 'approved'
                    AND a.attempt_count < $3
                    AND a.approval_revision > 0
                    AND length(a.authority_sha256) = 64
                    AND a.approved_authority_sha256 = a.authority_sha256
                    AND a.application_id = $4 AND c.provider = $5
                    AND NOT EXISTS (
                      SELECT 1 FROM account_deletion_intents deletion
                       WHERE deletion.account_id = a.account_id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM jobs_communication_write_fences fence
                       WHERE fence.account_id = a.account_id
                         AND fence.connection_id IN ('', a.connection_id)
                    )
                    AND (
                      a.kind <> 'reply' OR EXISTS (
                        SELECT 1 FROM jobs_provider_messages message
                         WHERE message.id = a.source_message_id
                           AND message.account_id = a.account_id
                           AND message.connection_id = a.connection_id
                           AND message.application_id = a.application_id
                      )
                    )
                    AND (
                      (a.provider IN ('gmail', 'google_calendar') AND c.provider = 'gmail')
                      OR (a.provider IN ('outlook_email', 'outlook_calendar')
                          AND c.provider = 'outlook')
                    )
                  FOR UPDATE OF a, c, credential",
                &[
                    &action_id,
                    &account_id,
                    &COMMUNICATION_ACTION_MAX_ATTEMPTS,
                    &application_id,
                    &mailbox_provider,
                ],
            )?;
            let Some(locked) = locked else {
                tx.commit()?;
                compare_exchange_operational_hold_scan_cursor(
                    &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                    initial_scan_cursor.as_ref(),
                    next_scan_cursor,
                );
                return Ok(None);
            };
            let fence = locked
                .get::<_, i64>(0)
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            let mailbox: MailboxConnection = parse_json(locked.get(1), "mailbox connection")?;
            if mailbox.provider != locked.get::<_, String>(3) {
                anyhow::bail!("communication mailbox authority changed")
            }
            require_communication_write_unfenced_postgres_tx(&mut tx, &account_id, &mailbox.id)?;
            let credential: JobsProviderCredential =
                parse_json(locked.get(2), "Jobs provider credential")?;
            let current = tx.query_one(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &action_id],
            )?;
            let current = communication_action_from_row(postgres_communication_row(current))?;
            validate_communication_kind_provider(&current.kind, &current.provider)?;
            validate_communication_payload(&current.kind, &current.payload)?;
            if current.kind == "reply" {
                let source_message_id = current
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_row = tx.query_one(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = $1 AND connection_id = $2 AND id = $3
                        AND application_id = $4 FOR SHARE",
                    &[
                        &account_id,
                        &current.connection_id,
                        &source_message_id,
                        &current.application_id,
                    ],
                )?;
                let source: JobsProviderMessage =
                    parse_json(source_row.get(0), "Jobs provider message")?;
                validate_communication_reply_source(&current, &source)?;
            }
            let authority_sha256 = communication_authority_sha256(&account_id, &current)?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&current.provider, &mailbox, &credential)?;
            if current.authority_sha256 != authority_sha256
                || current.approved_authority_sha256 != authority_sha256
                || current.approved_grant_revision != grant_revision
                || current.approved_grant_sha256 != grant_sha256
            {
                anyhow::bail!("communication action approval or provider grant changed")
            }
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
            if current.next_attempt_at_ms > effect_now_ms
                || current.attempt_count >= COMMUNICATION_ACTION_MAX_ATTEMPTS
            {
                tx.commit()?;
                compare_exchange_operational_hold_scan_cursor(
                    &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                    initial_scan_cursor.as_ref(),
                    next_scan_cursor,
                );
                return Ok(None);
            }
            let effect_expires_at_ms = effect_now_ms.saturating_add(COMMUNICATION_ACTION_LEASE_MS);
            tx.execute(
                "INSERT INTO jobs_communication_action_attempts (
                    id, account_id, action_id, connection_id, provider, dispatch_no, fence,
                    approval_revision, authority_sha256, grant_revision, grant_sha256,
                    provider_operation_key, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                &[
                    &attempt_id,
                    &account_id,
                    &action_id,
                    &current.connection_id,
                    &current.provider,
                    &(current.attempt_count + 1),
                    &fence,
                    &current.approval_revision,
                    &authority_sha256,
                    &grant_revision,
                    &grant_sha256,
                    &provider_operation_key,
                    &effect_now_ms,
                ],
            )?;
            let row = tx.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET status = 'dispatching', lease_owner = $2, lease_kind = 'dispatch',
                            lease_token_sha256 = $3, fence = $4, lease_expires_at_ms = $5,
                            active_attempt_id = $6,
                            attempt_count = attempt_count + 1,
                            action_revision = action_revision + 1,
                            dispatched_at_ms = COALESCE(dispatched_at_ms, $1),
                            updated_at_ms = GREATEST($1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE id = $7 AND account_id = $8 AND status = 'approved'
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &effect_now_ms,
                    &owner_id,
                    &token_hash,
                    &fence,
                    &effect_expires_at_ms,
                    &attempt_id,
                    &action_id,
                    &account_id,
                ],
            )?;
            tx.commit()?;
            compare_exchange_operational_hold_scan_cursor(
                &COMMUNICATION_OPERATIONAL_HOLD_SCAN_CURSORS,
                initial_scan_cursor.as_ref(),
                next_scan_cursor,
            );
            Ok(Some(JobsCommunicationActionLease {
                account_id,
                action: communication_action_from_row(postgres_communication_row(row))?,
                attempt_id,
                lease_kind: "dispatch".to_string(),
                provider_operation_key,
                authority_sha256,
                approval_revision: current.approval_revision,
                grant_revision,
                grant_sha256,
                lease_token,
                fence,
            }))
        }
    })
}

fn validate_communication_lease_binding(
    action: &JobsCommunicationAction,
    lease: &JobsCommunicationLeaseAccess,
    expected_kind: &str,
) -> Result<()> {
    if action.id != lease.action_id
        || action.active_attempt_id.as_deref() != Some(lease.attempt_id.as_str())
        || action.lease_owner.as_deref() != Some(lease.owner_id.as_str())
        || action.lease_kind.as_deref() != Some(expected_kind)
        || action.authority_sha256 != lease.authority_sha256
        || action.approval_revision != lease.approval_revision
        || action.approved_authority_sha256 != lease.authority_sha256
        || action.approved_grant_revision != lease.grant_revision
        || action.approved_grant_sha256 != lease.grant_sha256
    {
        anyhow::bail!("communication action lease authority changed")
    }
    Ok(())
}

fn validate_communication_outcome(
    action: &JobsCommunicationAction,
    outcome: &str,
    provider_object_id: &str,
    evidence: &Value,
) -> Result<()> {
    if !evidence.is_object() || evidence.as_object().is_some_and(serde_json::Map::is_empty) {
        anyhow::bail!("communication action completion needs provider evidence")
    }
    if provider_object_id.len() > 2_048 || provider_object_id.chars().any(char::is_control) {
        anyhow::bail!("communication provider object identity is invalid")
    }
    match outcome {
        "sent" if action.kind == "reply" && !provider_object_id.is_empty() => {
            validate_communication_success_evidence(action, provider_object_id, evidence)
        }
        "calendar_created" if action.kind == "calendar" && !provider_object_id.is_empty() => {
            validate_communication_success_evidence(action, provider_object_id, evidence)
        }
        "needs_input"
            if provider_object_id.is_empty()
                && evidence.get("no_side_effect").and_then(Value::as_bool) == Some(true) =>
        {
            Ok(())
        }
        "side_effect_unknown"
            if provider_object_id.is_empty()
                && evidence.get("no_side_effect").and_then(Value::as_bool) != Some(true) =>
        {
            Ok(())
        }
        "sent" | "calendar_created" => {
            anyhow::bail!("communication success does not match its exact action kind")
        }
        "failed" => anyhow::bail!("new communication failures must return to reviewed input"),
        _ => anyhow::bail!("invalid communication action outcome"),
    }
}

fn validate_communication_success_evidence(
    action: &JobsCommunicationAction,
    provider_object_id: &str,
    evidence: &Value,
) -> Result<()> {
    let action_id_sha256 = hex::encode(Sha256::digest(action.id.as_bytes()));
    if evidence.get("provider").and_then(Value::as_str) != Some(action.provider.as_str())
        || evidence.get("provider_object_id").and_then(Value::as_str) != Some(provider_object_id)
        || evidence.get("payload_sha256").and_then(Value::as_str)
            != Some(action.payload_sha256.as_str())
        || evidence.get("action_id_sha256").and_then(Value::as_str)
            != Some(action_id_sha256.as_str())
    {
        anyhow::bail!("communication success evidence does not match its exact action")
    }
    Ok(())
}

fn communication_operation_message_id(provider_operation_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(provider_operation_key.as_bytes()));
    format!("<{digest}@actions.bluey.sh>")
}

fn communication_google_event_id(provider_operation_key: &str) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";
    let digest = Sha256::digest(provider_operation_key.as_bytes());
    let mut output = String::with_capacity(32);
    let mut accumulator = 0u32;
    let mut bits = 0u8;
    for byte in digest {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 && output.len() < 32 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 31) as usize] as char);
        }
        if output.len() == 32 {
            break;
        }
    }
    output
}

fn communication_microsoft_transaction_id(provider_operation_key: &str) -> String {
    let digest = Sha256::digest(provider_operation_key.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn validate_communication_provider_success_proof(
    action: &JobsCommunicationAction,
    provider_operation_key: &str,
    provider_object_id: &str,
    evidence: &Value,
    source: Option<&JobsProviderMessage>,
) -> Result<()> {
    let operation_key_sha256 = hex::encode(Sha256::digest(provider_operation_key.as_bytes()));
    let valid = match action.provider.as_str() {
        "gmail" | "outlook_email" => {
            let operation_message_id = communication_operation_message_id(provider_operation_key);
            let (metadata_key, evidence_key) = if action.provider == "gmail" {
                ("thread_id", "thread_sha256")
            } else {
                ("conversation_id", "conversation_sha256")
            };
            let expected_source_sha256 = source
                .and_then(|value| value.metadata.get(metadata_key))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| hex::encode(Sha256::digest(value.as_bytes())));
            evidence.get("operation_message_id").and_then(Value::as_str)
                == Some(operation_message_id.as_str())
                && expected_source_sha256.as_deref().is_some_and(|expected| {
                    evidence.get(evidence_key).and_then(Value::as_str) == Some(expected)
                })
        }
        "google_calendar" => {
            let event_id = communication_google_event_id(provider_operation_key);
            provider_object_id == event_id
                && evidence
                    .get("deterministic_event_id")
                    .and_then(Value::as_str)
                    == Some(event_id.as_str())
                && evidence
                    .get("private_marker_sha256")
                    .and_then(Value::as_str)
                    == Some(operation_key_sha256.as_str())
        }
        "outlook_calendar" => {
            let transaction_id = communication_microsoft_transaction_id(provider_operation_key);
            evidence.get("transaction_id").and_then(Value::as_str) == Some(transaction_id.as_str())
                && evidence
                    .get("extended_property_sha256")
                    .and_then(Value::as_str)
                    == Some(operation_key_sha256.as_str())
        }
        _ => false,
    };
    if !valid {
        anyhow::bail!("communication provider success proof does not match its exact attempt")
    }
    Ok(())
}

fn communication_reply_source_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<Option<JobsProviderMessage>> {
    if action.kind != "reply" {
        return Ok(None);
    }
    let source_id = action
        .source_message_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
    let source_json: String = tx.query_row(
        "SELECT message_json FROM jobs_provider_messages
          WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
            AND application_id = ?4",
        params![
            account_id,
            action.connection_id,
            source_id,
            action.application_id,
        ],
        |row| row.get(0),
    )?;
    let source = parse_json(source_json, "Jobs provider message")?;
    validate_communication_reply_source(action, &source)?;
    Ok(Some(source))
}

fn communication_reply_source_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    action: &JobsCommunicationAction,
) -> Result<Option<JobsProviderMessage>> {
    if action.kind != "reply" {
        return Ok(None);
    }
    let source_id = action
        .source_message_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
    let source_json: String = tx
        .query_one(
            "SELECT message_json FROM jobs_provider_messages
              WHERE account_id = $1 AND connection_id = $2 AND id = $3
                AND application_id = $4 FOR SHARE",
            &[
                &account_id,
                &action.connection_id,
                &source_id,
                &action.application_id,
            ],
        )?
        .get(0);
    let source = parse_json(source_json, "Jobs provider message")?;
    validate_communication_reply_source(action, &source)?;
    Ok(Some(source))
}

fn validate_communication_reconciliation_evidence(
    action: &JobsCommunicationAction,
    provider_operation_key: &str,
    evidence: &Value,
) -> Result<()> {
    let action_id_sha256 = hex::encode(Sha256::digest(action.id.as_bytes()));
    let provider_operation_key_sha256 =
        hex::encode(Sha256::digest(provider_operation_key.as_bytes()));
    if evidence.get("provider").and_then(Value::as_str) != Some(action.provider.as_str())
        || evidence.get("action_id_sha256").and_then(Value::as_str)
            != Some(action_id_sha256.as_str())
        || evidence.get("payload_sha256").and_then(Value::as_str)
            != Some(action.payload_sha256.as_str())
        || evidence
            .get("provider_operation_key_sha256")
            .and_then(Value::as_str)
            != Some(provider_operation_key_sha256.as_str())
        || evidence.get("provider_operation_key").is_some()
    {
        anyhow::bail!("communication reconciliation evidence does not match its exact attempt")
    }
    Ok(())
}

pub fn mark_communication_action_request_started(
    pool: &DbPool,
    lease: &JobsCommunicationLeaseAccess,
) -> Result<JobsCommunicationAction> {
    if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
        anyhow::bail!("communication execution is not enabled")
    }
    let now = now_ms();
    let evidence = serde_json::json!({
        "authority_sha256": lease.authority_sha256,
        "approval_revision": lease.approval_revision,
        "grant_revision": lease.grant_revision,
        "grant_sha256": lease.grant_sha256,
    });
    let evidence_sha256 = communication_evidence_sha256(&evidence)?;
    let evidence_json = to_json(&evidence, "Jobs communication request-start evidence")?;
    let action = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &lease.account_id,
            )?;
            let connection_id: String = tx.query_row(
                "SELECT connection_id FROM jobs_communication_actions
                  WHERE account_id = ?1 AND id = ?2",
                params![lease.account_id, lease.action_id],
                |row| row.get(0),
            )?;
            require_communication_write_unfenced_sqlite_tx(&tx, &lease.account_id, &connection_id)?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            let (row, token_hash): (CommunicationActionRow, String) = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                      FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2 AND status = 'dispatching'
                        AND lease_kind = 'dispatch' AND fence = ?3
                        AND lease_expires_at_ms > ?4"
                ),
                params![lease.account_id, lease.action_id, lease.fence, now],
                |row| Ok((sqlite_communication_row(row)?, row.get(29)?)),
            )?;
            let action = communication_action_from_row(row)?;
            validate_communication_lease_binding(&action, lease, "dispatch")?;
            if !execution_lease_token_matches(&token_hash, &lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            validate_communication_kind_provider(&action.kind, &action.provider)?;
            validate_communication_payload(&action.kind, &action.payload)?;
            let authority_sha256 = communication_authority_sha256(&lease.account_id, &action)?;
            let (mailbox_json, credential_json, mailbox_provider): (String, String, String) = tx
                .query_row(
                    "SELECT mailbox.connection_json, credential.credential_json, mailbox.provider
                   FROM jobs_mailbox_connections mailbox
                   JOIN jobs_provider_credentials credential
                     ON credential.account_id = mailbox.account_id
                    AND credential.connection_id = mailbox.id
                  WHERE mailbox.account_id = ?1 AND mailbox.id = ?2
                    AND mailbox.status = 'connected'",
                    params![lease.account_id, action.connection_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
            let mailbox: MailboxConnection = parse_json(mailbox_json, "mailbox connection")?;
            if mailbox.provider != mailbox_provider {
                anyhow::bail!("communication mailbox authority changed")
            }
            let credential: JobsProviderCredential =
                parse_json(credential_json, "Jobs provider credential")?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&action.provider, &mailbox, &credential)?;
            if authority_sha256 != lease.authority_sha256
                || grant_revision != lease.grant_revision
                || grant_sha256 != lease.grant_sha256
            {
                anyhow::bail!("communication request-start authority changed")
            }
            if action.kind == "reply" {
                let source_id = action
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_json: String = tx.query_row(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = ?1 AND connection_id = ?2 AND id = ?3
                        AND application_id = ?4",
                    params![
                        lease.account_id,
                        action.connection_id,
                        source_id,
                        action.application_id,
                    ],
                    |row| row.get(0),
                )?;
                let source: JobsProviderMessage = parse_json(source_json, "Jobs provider message")?;
                validate_communication_reply_source(&action, &source)?;
            }
            let employer_domain = communication_employer_domain_sqlite_tx(
                &tx,
                &lease.account_id,
                &action.application_id,
            )?;
            let existing_evidence_sha256 = tx
                .query_row(
                    "SELECT evidence_sha256
                       FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                    params![lease.attempt_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(existing_evidence_sha256) = existing_evidence_sha256 {
                if existing_evidence_sha256 != evidence_sha256 {
                    anyhow::bail!("communication request-start evidence changed")
                }
                tx.commit()?;
                return Ok(Some(action));
            }
            if communication_dispatch_is_held_sqlite_tx_after_authority(
                &tx,
                &lease.account_id,
                &action.application_id,
                &mailbox_provider,
                &employer_domain,
            )? {
                let changed = tx.execute(
                    "UPDATE jobs_communication_actions
                        SET status = 'needs_input', lease_owner = NULL, lease_kind = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            active_attempt_id = NULL, approved_authority_sha256 = '',
                            approved_grant_revision = 0, approved_grant_sha256 = '',
                            approved_at_ms = NULL, next_attempt_at_ms = ?1,
                            action_revision = action_revision + 1,
                            updated_at_ms = MAX(?1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = ?2 AND id = ?3 AND status = 'dispatching'
                        AND lease_kind = 'dispatch' AND fence = ?4
                        AND lease_token_sha256 = ?5
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807",
                    params![
                        now,
                        lease.account_id,
                        lease.action_id,
                        lease.fence,
                        token_hash,
                    ],
                )?;
                if changed != 1 {
                    anyhow::bail!("communication action lease changed")
                }
                tx.commit()?;
                return Ok(None);
            }
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO jobs_communication_action_attempt_evidence (
                    id, account_id, action_id, attempt_id, event_kind, provider_object_id,
                    evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, 'request_started', NULL, ?5, ?6, ?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    lease.account_id,
                    lease.action_id,
                    lease.attempt_id,
                    evidence_sha256,
                    evidence_json,
                    now,
                ],
            )?;
            if inserted == 0 {
                let exact: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1
                       FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = ?1 AND event_kind = 'request_started'
                        AND evidence_sha256 = ?2)",
                    params![lease.attempt_id, evidence_sha256],
                    |row| row.get(0),
                )?;
                if !exact {
                    anyhow::bail!("communication request-start evidence changed")
                }
            }
            tx.commit()?;
            Ok(Some(action))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx)?;
            lock_discovery_account_shared_postgres(&mut tx, &lease.account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &lease.account_id,
            )?;
            let connection_id: String = tx
                .query_one(
                    "SELECT connection_id FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2",
                    &[&lease.account_id, &lease.action_id],
                )?
                .get(0);
            require_communication_write_unfenced_postgres_tx(
                &mut tx,
                &lease.account_id,
                &connection_id,
            )?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED") {
                anyhow::bail!("communication execution is not enabled")
            }
            let row = tx
                .query_opt(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                      FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2 AND status = 'dispatching'
                        AND lease_kind = 'dispatch' AND fence = $3
                      FOR UPDATE"
                    ),
                    &[&lease.account_id, &lease.action_id, &lease.fence],
                )?
                .ok_or_else(|| anyhow::anyhow!("communication action lease not found"))?;
            let token_hash: String = row.get(29);
            let action = communication_action_from_row(postgres_communication_row(row))?;
            validate_communication_lease_binding(&action, lease, "dispatch")?;
            if !execution_lease_token_matches(&token_hash, &lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            validate_communication_kind_provider(&action.kind, &action.provider)?;
            validate_communication_payload(&action.kind, &action.payload)?;
            let authority_sha256 = communication_authority_sha256(&lease.account_id, &action)?;
            let grant_row = tx.query_one(
                "SELECT mailbox.connection_json, credential.credential_json, mailbox.provider
                   FROM jobs_mailbox_connections mailbox
                   JOIN jobs_provider_credentials credential
                     ON credential.account_id = mailbox.account_id
                    AND credential.connection_id = mailbox.id
                  WHERE mailbox.account_id = $1 AND mailbox.id = $2
                    AND mailbox.status = 'connected'
                  FOR UPDATE OF mailbox, credential",
                &[&lease.account_id, &action.connection_id],
            )?;
            let mailbox: MailboxConnection = parse_json(grant_row.get(0), "mailbox connection")?;
            let mailbox_provider = grant_row.get::<_, String>(2);
            if mailbox.provider != mailbox_provider {
                anyhow::bail!("communication mailbox authority changed")
            }
            let credential: JobsProviderCredential =
                parse_json(grant_row.get(1), "Jobs provider credential")?;
            let (grant_revision, grant_sha256) =
                validate_communication_grant(&action.provider, &mailbox, &credential)?;
            if authority_sha256 != lease.authority_sha256
                || grant_revision != lease.grant_revision
                || grant_sha256 != lease.grant_sha256
            {
                anyhow::bail!("communication request-start authority changed")
            }
            if action.kind == "reply" {
                let source_id = action
                    .source_message_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("communication reply source is missing"))?;
                let source_row = tx.query_one(
                    "SELECT message_json FROM jobs_provider_messages
                      WHERE account_id = $1 AND connection_id = $2 AND id = $3
                        AND application_id = $4 FOR SHARE",
                    &[
                        &lease.account_id,
                        &action.connection_id,
                        &source_id,
                        &action.application_id,
                    ],
                )?;
                let source: JobsProviderMessage =
                    parse_json(source_row.get(0), "Jobs provider message")?;
                validate_communication_reply_source(&action, &source)?;
            }
            let employer_domain =
                communication_employer_domain_postgres_tx_after_authority_prelock(
                    &mut tx,
                    &lease.account_id,
                    &action.application_id,
                )?;
            let existing_evidence_sha256 = tx
                .query_opt(
                    "SELECT evidence_sha256
                       FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = $1 AND event_kind = 'request_started'
                      FOR SHARE",
                    &[&lease.attempt_id],
                )?
                .map(|row| row.get::<_, String>(0));
            if let Some(existing_evidence_sha256) = existing_evidence_sha256 {
                let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
                require_communication_lease_current_at_ms(&action, effect_now_ms)?;
                if existing_evidence_sha256 != evidence_sha256 {
                    anyhow::bail!("communication request-start evidence changed")
                }
                tx.commit()?;
                return Ok(Some(action));
            }
            let dispatch_held =
                communication_dispatch_is_held_postgres_tx_with_domain_after_authority_prelock(
                    &mut tx,
                    &lease.account_id,
                    &action.application_id,
                    &mailbox_provider,
                    &employer_domain,
                )?;
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
            require_communication_lease_current_at_ms(&action, effect_now_ms)?;
            if dispatch_held {
                let changed = tx.execute(
                    "UPDATE jobs_communication_actions
                        SET status = 'needs_input', lease_owner = NULL, lease_kind = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            active_attempt_id = NULL, approved_authority_sha256 = '',
                            approved_grant_revision = 0, approved_grant_sha256 = '',
                            approved_at_ms = NULL, next_attempt_at_ms = $1,
                            action_revision = action_revision + 1,
                            updated_at_ms = GREATEST($1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = $2 AND id = $3 AND status = 'dispatching'
                        AND lease_kind = 'dispatch' AND fence = $4
                        AND lease_token_sha256 = $5
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807",
                    &[
                        &effect_now_ms,
                        &lease.account_id,
                        &lease.action_id,
                        &lease.fence,
                        &token_hash,
                    ],
                )?;
                if changed != 1 {
                    anyhow::bail!("communication action lease changed")
                }
                tx.commit()?;
                return Ok(None);
            }
            let inserted = tx.execute(
                "INSERT INTO jobs_communication_action_attempt_evidence (
                    id, account_id, action_id, attempt_id, event_kind, provider_object_id,
                    evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, 'request_started', NULL, $5, $6, $7)
                 ON CONFLICT(attempt_id, event_kind) DO NOTHING",
                &[
                    &uuid::Uuid::new_v4().to_string(),
                    &lease.account_id,
                    &lease.action_id,
                    &lease.attempt_id,
                    &evidence_sha256,
                    &evidence_json,
                    &effect_now_ms,
                ],
            )?;
            if inserted == 0 {
                let exact: bool = tx
                    .query_one(
                        "SELECT EXISTS(SELECT 1
                       FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = $1 AND event_kind = 'request_started'
                        AND evidence_sha256 = $2)",
                        &[&lease.attempt_id, &evidence_sha256],
                    )?
                    .get(0);
                if !exact {
                    anyhow::bail!("communication request-start evidence changed")
                }
            }
            tx.commit()?;
            Ok(Some(action))
        }
    })?;
    action.ok_or_else(|| anyhow::anyhow!("communication dispatch is unavailable"))
}

pub fn finish_communication_action(
    pool: &DbPool,
    finish: &JobsCommunicationActionFinish,
) -> Result<JobsCommunicationAction> {
    let outcome = finish.outcome.trim().to_ascii_lowercase();
    let provider_object_id = finish.provider_object_id.trim();
    if provider_object_id != finish.provider_object_id {
        anyhow::bail!("communication provider object identity is not canonical")
    }
    let evidence = canonical_communication_value(&finish.evidence);
    let evidence_sha256 = communication_evidence_sha256(&evidence)?;
    let evidence_json = to_json(&evidence, "Jobs communication completion evidence")?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (row, token_hash): (CommunicationActionRow, String) = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                       FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2 AND fence = ?3
                        AND status IN ('dispatching', 'side_effect_unknown')
                        AND lease_kind = 'dispatch' AND lease_expires_at_ms > ?4"
                ),
                params![
                    finish.lease.account_id,
                    finish.lease.action_id,
                    finish.lease.fence,
                    now,
                ],
                |row| Ok((sqlite_communication_row(row)?, row.get(29)?)),
            )?;
            let action = communication_action_from_row(row)?;
            validate_communication_lease_binding(&action, &finish.lease, "dispatch")?;
            if !execution_lease_token_matches(&token_hash, &finish.lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            let provider_operation_key: String = tx.query_row(
                "SELECT provider_operation_key FROM jobs_communication_action_attempts
                  WHERE id = ?1 AND account_id = ?2 AND action_id = ?3 AND fence = ?4
                    AND approval_revision = ?5 AND authority_sha256 = ?6
                    AND grant_revision = ?7 AND grant_sha256 = ?8",
                params![
                    finish.lease.attempt_id,
                    finish.lease.account_id,
                    finish.lease.action_id,
                    finish.lease.fence,
                    finish.lease.approval_revision,
                    finish.lease.authority_sha256,
                    finish.lease.grant_revision,
                    finish.lease.grant_sha256,
                ],
                |row| row.get(0),
            )?;
            validate_communication_outcome(&action, &outcome, provider_object_id, &evidence)?;
            if matches!(outcome.as_str(), "sent" | "calendar_created") {
                let source =
                    communication_reply_source_sqlite_tx(&tx, &finish.lease.account_id, &action)?;
                validate_communication_reconciliation_evidence(
                    &action,
                    &provider_operation_key,
                    &evidence,
                )?;
                validate_communication_provider_success_proof(
                    &action,
                    &provider_operation_key,
                    provider_object_id,
                    &evidence,
                    source.as_ref(),
                )?;
            }
            let request_started: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = ?1 AND event_kind = 'request_started')",
                params![finish.lease.attempt_id],
                |row| row.get(0),
            )?;
            if matches!(
                outcome.as_str(),
                "sent" | "calendar_created" | "side_effect_unknown"
            ) && !request_started
            {
                anyhow::bail!("communication provider request was not durably started")
            }
            tx.execute(
                "INSERT INTO jobs_communication_action_attempt_evidence (
                    id, account_id, action_id, attempt_id, event_kind, provider_object_id,
                    evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULLIF(?6, ''), ?7, ?8, ?9)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    finish.lease.account_id,
                    finish.lease.action_id,
                    finish.lease.attempt_id,
                    outcome,
                    provider_object_id,
                    evidence_sha256,
                    evidence_json,
                    now,
                ],
            )?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = ?4, provider_object_id = NULLIF(?5, ''),
                        lease_owner = NULL, lease_kind = NULL, lease_token_sha256 = NULL,
                        lease_expires_at_ms = NULL,
                        action_revision = action_revision + 1,
                        active_attempt_id = CASE WHEN ?4 = 'needs_input' THEN NULL
                                                 ELSE active_attempt_id END,
                        approved_authority_sha256 = CASE WHEN ?4 = 'needs_input' THEN ''
                                                         ELSE approved_authority_sha256 END,
                        approved_grant_revision = CASE WHEN ?4 = 'needs_input' THEN 0
                                                       ELSE approved_grant_revision END,
                        approved_grant_sha256 = CASE WHEN ?4 = 'needs_input' THEN ''
                                                    ELSE approved_grant_sha256 END,
                        approved_at_ms = CASE WHEN ?4 = 'needs_input' THEN NULL
                                              ELSE approved_at_ms END,
                        updated_at_ms = MAX(?6, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?1 AND id = ?2 AND fence = ?3
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![
                    finish.lease.account_id,
                    finish.lease.action_id,
                    finish.lease.fence,
                    outcome,
                    provider_object_id,
                    now,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("communication completion lease changed")
            }
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = ?1 AND id = ?2"
                ),
                params![finish.lease.account_id, finish.lease.action_id],
                sqlite_communication_row,
            )?;
            tx.commit()?;
            communication_action_from_row(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx
                .query_opt(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                       FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2 AND fence = $3
                        AND status IN ('dispatching', 'side_effect_unknown')
                        AND lease_kind = 'dispatch' FOR UPDATE"
                    ),
                    &[
                        &finish.lease.account_id,
                        &finish.lease.action_id,
                        &finish.lease.fence,
                    ],
                )?
                .ok_or_else(|| anyhow::anyhow!("communication action lease not found"))?;
            let token_hash: String = row.get(29);
            let action = communication_action_from_row(postgres_communication_row(row))?;
            validate_communication_lease_binding(&action, &finish.lease, "dispatch")?;
            if !execution_lease_token_matches(&token_hash, &finish.lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            let provider_operation_key: String = tx
                .query_one(
                    "SELECT provider_operation_key FROM jobs_communication_action_attempts
                      WHERE id = $1 AND account_id = $2 AND action_id = $3 AND fence = $4
                        AND approval_revision = $5 AND authority_sha256 = $6
                        AND grant_revision = $7 AND grant_sha256 = $8",
                    &[
                        &finish.lease.attempt_id,
                        &finish.lease.account_id,
                        &finish.lease.action_id,
                        &finish.lease.fence,
                        &finish.lease.approval_revision,
                        &finish.lease.authority_sha256,
                        &finish.lease.grant_revision,
                        &finish.lease.grant_sha256,
                    ],
                )?
                .get(0);
            validate_communication_outcome(&action, &outcome, provider_object_id, &evidence)?;
            if matches!(outcome.as_str(), "sent" | "calendar_created") {
                let source = communication_reply_source_postgres_tx(
                    &mut tx,
                    &finish.lease.account_id,
                    &action,
                )?;
                validate_communication_reconciliation_evidence(
                    &action,
                    &provider_operation_key,
                    &evidence,
                )?;
                validate_communication_provider_success_proof(
                    &action,
                    &provider_operation_key,
                    provider_object_id,
                    &evidence,
                    source.as_ref(),
                )?;
            }
            let request_started: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = $1 AND event_kind = 'request_started')",
                    &[&finish.lease.attempt_id],
                )?
                .get(0);
            if matches!(
                outcome.as_str(),
                "sent" | "calendar_created" | "side_effect_unknown"
            ) && !request_started
            {
                anyhow::bail!("communication provider request was not durably started")
            }
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
            require_communication_lease_current_at_ms(&action, effect_now_ms)?;
            tx.execute(
                "INSERT INTO jobs_communication_action_attempt_evidence (
                    id, account_id, action_id, attempt_id, event_kind, provider_object_id,
                    evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, NULLIF($6, ''), $7, $8, $9)",
                &[
                    &uuid::Uuid::new_v4().to_string(),
                    &finish.lease.account_id,
                    &finish.lease.action_id,
                    &finish.lease.attempt_id,
                    &outcome,
                    &provider_object_id,
                    &evidence_sha256,
                    &evidence_json,
                    &effect_now_ms,
                ],
            )?;
            let row = tx.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET status = $4, provider_object_id = NULLIF($5, ''),
                            lease_owner = NULL, lease_kind = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            action_revision = action_revision + 1,
                            active_attempt_id = CASE WHEN $4 = 'needs_input' THEN NULL
                                                     ELSE active_attempt_id END,
                            approved_authority_sha256 = CASE WHEN $4 = 'needs_input' THEN ''
                                                             ELSE approved_authority_sha256 END,
                            approved_grant_revision = CASE WHEN $4 = 'needs_input' THEN 0
                                                           ELSE approved_grant_revision END,
                            approved_grant_sha256 = CASE WHEN $4 = 'needs_input' THEN ''
                                                        ELSE approved_grant_sha256 END,
                            approved_at_ms = CASE WHEN $4 = 'needs_input' THEN NULL
                                                  ELSE approved_at_ms END,
                            updated_at_ms = GREATEST($6, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = $1 AND id = $2 AND fence = $3
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &finish.lease.account_id,
                    &finish.lease.action_id,
                    &finish.lease.fence,
                    &outcome,
                    &provider_object_id,
                    &effect_now_ms,
                ],
            )?;
            tx.commit()?;
            communication_action_from_row(postgres_communication_row(row))
        }
    })
}

pub fn claim_communication_action_reconciliation(
    pool: &DbPool,
    owner_id: &str,
) -> Result<Option<JobsCommunicationActionLease>> {
    if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
        anyhow::bail!("communication reconciliation is not enabled")
    }
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
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
                anyhow::bail!("communication reconciliation is not enabled")
            }
            let candidate: Option<CommunicationReconciliationCandidateRow> = tx
                .query_row(
                    "SELECT a.id, a.account_id, attempt.id, a.fence,
                            attempt.provider_operation_key, attempt.approval_revision,
                            attempt.authority_sha256, attempt.grant_revision,
                            attempt.grant_sha256
                       FROM jobs_communication_actions a
                       JOIN jobs_communication_action_attempts attempt
                         ON attempt.id = a.active_attempt_id
                        AND attempt.account_id = a.account_id
                        AND attempt.action_id = a.id
                       JOIN jobs_mailbox_connections mailbox
                         ON mailbox.account_id = a.account_id
                        AND mailbox.id = a.connection_id
                        AND mailbox.status = 'connected'
                      WHERE a.status = 'side_effect_unknown'
                        AND a.reconciliation_count < 20
                        AND a.action_revision <= 9007199254740989
                        AND a.next_attempt_at_ms <= ?1
                        AND (a.lease_kind IS NULL OR a.lease_expires_at_ms <= ?1)
                        AND NOT EXISTS (
                          SELECT 1 FROM account_deletion_intents deletion
                           WHERE deletion.account_id = a.account_id
                        )
                      ORDER BY a.next_attempt_at_ms, a.updated_at_ms, a.id LIMIT 1",
                    params![now],
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
            let Some((
                action_id,
                account_id,
                attempt_id,
                current_fence,
                provider_operation_key,
                approval_revision,
                authority_sha256,
                grant_revision,
                grant_sha256,
            )) = candidate
            else {
                tx.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
            let fence = current_fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET lease_owner = ?2, lease_kind = 'reconcile', lease_token_sha256 = ?3,
                        fence = ?4, lease_expires_at_ms = ?5,
                        reconciliation_count = reconciliation_count + 1,
                        action_revision = action_revision + 1,
                        updated_at_ms = MAX(?1, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?6 AND id = ?7 AND status = 'side_effect_unknown'
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![now, owner_id, token_hash, fence, expires_at, account_id, action_id],
            )?;
            if changed != 1 {
                anyhow::bail!("communication reconciliation authority changed")
            }
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
                attempt_id,
                lease_kind: "reconcile".to_string(),
                provider_operation_key,
                authority_sha256,
                approval_revision,
                grant_revision,
                grant_sha256,
                lease_token,
                fence,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
                anyhow::bail!("communication reconciliation is not enabled")
            }
            let candidate = tx.query_opt(
                "SELECT a.id, a.account_id
                   FROM jobs_communication_actions a
                   JOIN jobs_communication_action_attempts attempt
                     ON attempt.id = a.active_attempt_id
                    AND attempt.account_id = a.account_id
                    AND attempt.action_id = a.id
                   JOIN jobs_mailbox_connections mailbox
                     ON mailbox.account_id = a.account_id
                    AND mailbox.id = a.connection_id
                    AND mailbox.status = 'connected'
                  WHERE a.status = 'side_effect_unknown'
                    AND a.reconciliation_count < 20
                    AND a.action_revision <= 9007199254740989
                    AND a.next_attempt_at_ms <= $1
                    AND (a.lease_kind IS NULL OR a.lease_expires_at_ms <= $1)
                    AND NOT EXISTS (
                      SELECT 1 FROM account_deletion_intents deletion
                       WHERE deletion.account_id = a.account_id
                    )
                  ORDER BY a.next_attempt_at_ms, a.updated_at_ms, a.id
                  LIMIT 1",
                &[&now],
            )?;
            let Some(candidate) = candidate else {
                tx.commit()?;
                return Ok(None);
            };
            let action_id: String = candidate.get(0);
            let account_id: String = candidate.get(1);
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let locked = tx.query_opt(
                "SELECT attempt.id, a.fence, attempt.provider_operation_key,
                        attempt.approval_revision, attempt.authority_sha256,
                        attempt.grant_revision, attempt.grant_sha256,
                        a.next_attempt_at_ms, a.lease_kind, a.lease_expires_at_ms
                   FROM jobs_communication_actions a
                   JOIN jobs_communication_action_attempts attempt
                     ON attempt.id = a.active_attempt_id
                    AND attempt.account_id = a.account_id
                    AND attempt.action_id = a.id
                   JOIN jobs_mailbox_connections mailbox
                     ON mailbox.account_id = a.account_id
                    AND mailbox.id = a.connection_id
                    AND mailbox.status = 'connected'
                  WHERE a.id = $1 AND a.account_id = $2
                    AND a.status = 'side_effect_unknown'
                    AND a.reconciliation_count < 20
                    AND a.action_revision <= 9007199254740989
                    AND NOT EXISTS (
                      SELECT 1 FROM account_deletion_intents deletion
                       WHERE deletion.account_id = a.account_id
                    )
                  FOR UPDATE OF a, mailbox",
                &[&action_id, &account_id],
            )?;
            let Some(locked) = locked else {
                tx.commit()?;
                return Ok(None);
            };
            let attempt_id: String = locked.get(0);
            let fence = locked
                .get::<_, i64>(1)
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("communication action fence overflow"))?;
            let provider_operation_key: String = locked.get(2);
            let approval_revision: i64 = locked.get(3);
            let authority_sha256: String = locked.get(4);
            let grant_revision: i64 = locked.get(5);
            let grant_sha256: String = locked.get(6);
            let next_attempt_at_ms: i64 = locked.get(7);
            let existing_lease_kind: Option<String> = locked.get(8);
            let existing_lease_expires_at_ms: Option<i64> = locked.get(9);
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
            let existing_lease_available =
                match (existing_lease_kind.as_deref(), existing_lease_expires_at_ms) {
                    (None, None) => true,
                    (Some(_), Some(expires_at_ms)) => expires_at_ms <= effect_now_ms,
                    _ => false,
                };
            if next_attempt_at_ms > effect_now_ms || !existing_lease_available {
                tx.commit()?;
                return Ok(None);
            }
            let effect_expires_at_ms = effect_now_ms.saturating_add(COMMUNICATION_ACTION_LEASE_MS);
            let row = tx.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET lease_owner = $2, lease_kind = 'reconcile',
                            lease_token_sha256 = $3, fence = $4, lease_expires_at_ms = $5,
                            reconciliation_count = reconciliation_count + 1,
                            action_revision = action_revision + 1,
                            updated_at_ms = GREATEST($1, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = $6 AND id = $7 AND status = 'side_effect_unknown'
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &effect_now_ms,
                    &owner_id,
                    &token_hash,
                    &fence,
                    &effect_expires_at_ms,
                    &account_id,
                    &action_id,
                ],
            )?;
            tx.commit()?;
            Ok(Some(JobsCommunicationActionLease {
                account_id,
                action: communication_action_from_row(postgres_communication_row(row))?,
                attempt_id,
                lease_kind: "reconcile".to_string(),
                provider_operation_key,
                authority_sha256,
                approval_revision,
                grant_revision,
                grant_sha256,
                lease_token,
                fence,
            }))
        }
    })
}

pub fn reconcile_communication_action(
    pool: &DbPool,
    reconciliation: &JobsCommunicationActionReconciliation,
) -> Result<JobsCommunicationAction> {
    if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
        anyhow::bail!("communication reconciliation is not enabled")
    }
    let resolution = reconciliation.resolution.trim().to_ascii_lowercase();
    let provider_object_id = reconciliation.provider_object_id.trim();
    if provider_object_id != reconciliation.provider_object_id
        || provider_object_id.len() > 2_048
        || provider_object_id.chars().any(char::is_control)
    {
        anyhow::bail!("communication provider object identity is invalid")
    }
    if matches!(resolution.as_str(), "confirmed_absent" | "inconclusive")
        && !provider_object_id.is_empty()
    {
        anyhow::bail!("communication absence evidence cannot name a provider object")
    }
    let evidence = canonical_communication_value(&reconciliation.evidence);
    if !evidence.is_object() || evidence.as_object().is_some_and(serde_json::Map::is_empty) {
        anyhow::bail!("communication reconciliation needs provider evidence")
    }
    if resolution == "confirmed_absent"
        && evidence
            .get("authoritative_absence")
            .and_then(Value::as_bool)
            != Some(true)
    {
        anyhow::bail!("communication absence needs authoritative provider evidence")
    }
    let evidence_sha256 = communication_evidence_sha256(&evidence)?;
    let evidence_json = to_json(&evidence, "Jobs communication reconciliation evidence")?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
                anyhow::bail!("communication reconciliation is not enabled")
            }
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &reconciliation.lease.account_id,
            )?;
            let (row, token_hash): (CommunicationActionRow, String) = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                      FROM jobs_communication_actions
                      WHERE account_id = ?1 AND id = ?2 AND fence = ?3
                        AND status = 'side_effect_unknown' AND lease_kind = 'reconcile'
                        AND lease_expires_at_ms > ?4"
                ),
                params![
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id,
                    reconciliation.lease.fence,
                    now,
                ],
                |row| Ok((sqlite_communication_row(row)?, row.get(29)?)),
            )?;
            let action = communication_action_from_row(row)?;
            tx.query_row(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = ?1 AND id = ?2 AND status = 'connected'",
                params![reconciliation.lease.account_id, action.connection_id],
                |_| Ok(()),
            )?;
            validate_communication_lease_binding(&action, &reconciliation.lease, "reconcile")?;
            let provider_operation_key: String = tx.query_row(
                "SELECT provider_operation_key
                   FROM jobs_communication_action_attempts
                  WHERE account_id = ?1 AND action_id = ?2 AND id = ?3",
                params![
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id,
                    reconciliation.lease.attempt_id,
                ],
                |row| row.get(0),
            )?;
            validate_communication_reconciliation_evidence(
                &action,
                &provider_operation_key,
                &evidence,
            )?;
            if resolution == "confirmed_absent"
                && action.dispatched_at_ms.is_none_or(|dispatched_at| {
                    now < dispatched_at
                        .saturating_add(COMMUNICATION_RECONCILIATION_ABSENCE_MIN_AGE_MS)
                })
            {
                anyhow::bail!("communication absence was checked before the minimum age")
            }
            if !execution_lease_token_matches(&token_hash, &reconciliation.lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            let prior_absences: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_action_reconciliations
                  WHERE account_id = ?1 AND action_id = ?2 AND attempt_id = ?3
                    AND resolution = 'confirmed_absent'",
                params![
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id,
                    reconciliation.lease.attempt_id,
                ],
                |row| row.get(0),
            )?;
            let status = match resolution.as_str() {
                "confirmed_sent" if action.kind == "reply" && !provider_object_id.is_empty() => {
                    "sent"
                }
                "confirmed_calendar"
                    if action.kind == "calendar" && !provider_object_id.is_empty() =>
                {
                    "calendar_created"
                }
                "confirmed_absent"
                    if prior_absences.saturating_add(1)
                        >= COMMUNICATION_RECONCILIATION_ABSENCE_THRESHOLD =>
                {
                    "needs_input"
                }
                "confirmed_absent" => "side_effect_unknown",
                "inconclusive" => "side_effect_unknown",
                _ => anyhow::bail!("invalid communication reconciliation resolution"),
            };
            if matches!(status, "sent" | "calendar_created") {
                let source = communication_reply_source_sqlite_tx(
                    &tx,
                    &reconciliation.lease.account_id,
                    &action,
                )?;
                validate_communication_success_evidence(&action, provider_object_id, &evidence)?;
                validate_communication_provider_success_proof(
                    &action,
                    &provider_operation_key,
                    provider_object_id,
                    &evidence,
                    source.as_ref(),
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_communication_action_reconciliations (
                    id, account_id, action_id, attempt_id, fence, resolution,
                    provider_object_id, evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULLIF(?7, ''), ?8, ?9, ?10)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id,
                    reconciliation.lease.attempt_id,
                    reconciliation.lease.fence,
                    resolution,
                    provider_object_id,
                    evidence_sha256,
                    evidence_json,
                    now,
                ],
            )?;
            let changed = tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = ?4, provider_object_id = NULLIF(?5, ''),
                        lease_owner = NULL, lease_kind = NULL, lease_token_sha256 = NULL,
                        lease_expires_at_ms = NULL,
                        action_revision = action_revision + 1,
                        active_attempt_id = CASE WHEN ?4 = 'needs_input' THEN NULL
                                                 ELSE active_attempt_id END,
                        approved_authority_sha256 = CASE WHEN ?4 = 'needs_input' THEN ''
                                                         ELSE approved_authority_sha256 END,
                        approved_grant_revision = CASE WHEN ?4 = 'needs_input' THEN 0
                                                       ELSE approved_grant_revision END,
                        approved_grant_sha256 = CASE WHEN ?4 = 'needs_input' THEN ''
                                                    ELSE approved_grant_sha256 END,
                        approved_at_ms = CASE WHEN ?4 = 'needs_input' THEN NULL
                                              ELSE approved_at_ms END,
                        next_attempt_at_ms = ?6,
                        updated_at_ms = MAX(?7, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?1 AND id = ?2 AND fence = ?3
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id,
                    reconciliation.lease.fence,
                    status,
                    provider_object_id,
                    if status == "side_effect_unknown" {
                        now.saturating_add(5 * 60_000)
                    } else {
                        now
                    },
                    now,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("communication reconciliation lease changed")
            }
            let row = tx.query_row(
                &format!(
                    "SELECT {COMMUNICATION_ACTION_SELECT}
                       FROM jobs_communication_actions WHERE account_id = ?1 AND id = ?2"
                ),
                params![
                    reconciliation.lease.account_id,
                    reconciliation.lease.action_id
                ],
                sqlite_communication_row,
            )?;
            tx.commit()?;
            communication_action_from_row(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if !communication_flag_enabled("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED") {
                anyhow::bail!("communication reconciliation is not enabled")
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &reconciliation.lease.account_id,
            )?;
            let row = tx
                .query_opt(
                    &format!(
                        "SELECT {COMMUNICATION_ACTION_SELECT}, lease_token_sha256
                      FROM jobs_communication_actions
                      WHERE account_id = $1 AND id = $2 AND fence = $3
                        AND status = 'side_effect_unknown' AND lease_kind = 'reconcile'
                      FOR UPDATE"
                    ),
                    &[
                        &reconciliation.lease.account_id,
                        &reconciliation.lease.action_id,
                        &reconciliation.lease.fence,
                    ],
                )?
                .ok_or_else(|| anyhow::anyhow!("communication action lease not found"))?;
            let token_hash: String = row.get(29);
            let action = communication_action_from_row(postgres_communication_row(row))?;
            tx.query_one(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2 AND status = 'connected' FOR SHARE",
                &[&reconciliation.lease.account_id, &action.connection_id],
            )?;
            validate_communication_lease_binding(&action, &reconciliation.lease, "reconcile")?;
            let provider_operation_key: String = tx
                .query_one(
                    "SELECT provider_operation_key
                       FROM jobs_communication_action_attempts
                      WHERE account_id = $1 AND action_id = $2 AND id = $3",
                    &[
                        &reconciliation.lease.account_id,
                        &reconciliation.lease.action_id,
                        &reconciliation.lease.attempt_id,
                    ],
                )?
                .get(0);
            validate_communication_reconciliation_evidence(
                &action,
                &provider_operation_key,
                &evidence,
            )?;
            if !execution_lease_token_matches(&token_hash, &reconciliation.lease.lease_token) {
                anyhow::bail!("communication action lease not found")
            }
            let prior_absences: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_action_reconciliations
                      WHERE account_id = $1 AND action_id = $2 AND attempt_id = $3
                        AND resolution = 'confirmed_absent'",
                    &[
                        &reconciliation.lease.account_id,
                        &reconciliation.lease.action_id,
                        &reconciliation.lease.attempt_id,
                    ],
                )?
                .get(0);
            let status = match resolution.as_str() {
                "confirmed_sent" if action.kind == "reply" && !provider_object_id.is_empty() => {
                    "sent"
                }
                "confirmed_calendar"
                    if action.kind == "calendar" && !provider_object_id.is_empty() =>
                {
                    "calendar_created"
                }
                "confirmed_absent"
                    if prior_absences.saturating_add(1)
                        >= COMMUNICATION_RECONCILIATION_ABSENCE_THRESHOLD =>
                {
                    "needs_input"
                }
                "confirmed_absent" => "side_effect_unknown",
                "inconclusive" => "side_effect_unknown",
                _ => anyhow::bail!("invalid communication reconciliation resolution"),
            };
            if matches!(status, "sent" | "calendar_created") {
                let source = communication_reply_source_postgres_tx(
                    &mut tx,
                    &reconciliation.lease.account_id,
                    &action,
                )?;
                validate_communication_success_evidence(&action, provider_object_id, &evidence)?;
                validate_communication_provider_success_proof(
                    &action,
                    &provider_operation_key,
                    provider_object_id,
                    &evidence,
                    source.as_ref(),
                )?;
            }
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)?;
            require_communication_lease_current_at_ms(&action, effect_now_ms)?;
            if resolution == "confirmed_absent"
                && action.dispatched_at_ms.is_none_or(|dispatched_at| {
                    effect_now_ms
                        < dispatched_at
                            .saturating_add(COMMUNICATION_RECONCILIATION_ABSENCE_MIN_AGE_MS)
                })
            {
                anyhow::bail!("communication absence was checked before the minimum age")
            }
            tx.execute(
                "INSERT INTO jobs_communication_action_reconciliations (
                    id, account_id, action_id, attempt_id, fence, resolution,
                    provider_object_id, evidence_sha256, evidence_json, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, NULLIF($7, ''), $8, $9, $10)",
                &[
                    &uuid::Uuid::new_v4().to_string(),
                    &reconciliation.lease.account_id,
                    &reconciliation.lease.action_id,
                    &reconciliation.lease.attempt_id,
                    &reconciliation.lease.fence,
                    &resolution,
                    &provider_object_id,
                    &evidence_sha256,
                    &evidence_json,
                    &effect_now_ms,
                ],
            )?;
            let next_attempt_at = if status == "side_effect_unknown" {
                effect_now_ms.saturating_add(5 * 60_000)
            } else {
                effect_now_ms
            };
            let row = tx.query_one(
                &format!(
                    "UPDATE jobs_communication_actions
                        SET status = $4, provider_object_id = NULLIF($5, ''),
                            lease_owner = NULL, lease_kind = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            action_revision = action_revision + 1,
                            active_attempt_id = CASE WHEN $4 = 'needs_input' THEN NULL
                                                     ELSE active_attempt_id END,
                            approved_authority_sha256 = CASE WHEN $4 = 'needs_input' THEN ''
                                                             ELSE approved_authority_sha256 END,
                            approved_grant_revision = CASE WHEN $4 = 'needs_input' THEN 0
                                                           ELSE approved_grant_revision END,
                            approved_grant_sha256 = CASE WHEN $4 = 'needs_input' THEN ''
                                                        ELSE approved_grant_sha256 END,
                            approved_at_ms = CASE WHEN $4 = 'needs_input' THEN NULL
                                                  ELSE approved_at_ms END,
                            next_attempt_at_ms = $6,
                            updated_at_ms = GREATEST($7, CASE
                              WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                              ELSE updated_at_ms END)
                      WHERE account_id = $1 AND id = $2 AND fence = $3
                        AND action_revision < 9007199254740991
                        AND updated_at_ms < 9223372036854775807
                      RETURNING {COMMUNICATION_ACTION_SELECT}"
                ),
                &[
                    &reconciliation.lease.account_id,
                    &reconciliation.lease.action_id,
                    &reconciliation.lease.fence,
                    &status,
                    &provider_object_id,
                    &next_attempt_at,
                    &effect_now_ms,
                ],
            )?;
            tx.commit()?;
            communication_action_from_row(postgres_communication_row(row))
        }
    })
}

#[cfg(test)]
#[test]
fn fix_728_communication_dispatch_uses_submitted_domain_after_one_canonical_prelock() {
    let source = include_str!("communication_actions.rs");
    let held = source
        .split("fn communication_dispatch_is_held_postgres_tx_after_authority_prelock(")
        .nth(1)
        .expect("PostgreSQL CommunicationDispatch hold helper")
        .split("fn communication_employer_domain_sqlite_tx(")
        .next()
        .expect("bounded PostgreSQL CommunicationDispatch hold helper");
    assert!(held.contains("communication_employer_domain_postgres_tx_after_authority_prelock"));
    assert!(held
        .contains("operational_hold_context_for_application_postgres_tx_after_authority_prelock"));
    assert!(held.contains("evaluate_operational_capability_postgres_tx_after_authority_prelock"));
    for forbidden in [
        "lock_operational_hold_shared_postgres_tx",
        "lock_managed_cloud_release_registry_shared_postgres_tx",
        "lock_postgres_ats_certification",
        "lock_discovery_account_shared_postgres",
        "operational_hold_context_for_application_postgres_tx(",
        "evaluate_operational_capability_postgres_tx(",
    ] {
        assert!(
            !held.contains(forbidden),
            "communication hold relocks {forbidden}"
        );
    }
    let historical_domain = source
        .split("fn communication_employer_domain_postgres_tx_after_authority_prelock(")
        .nth(1)
        .expect("PostgreSQL submitted communication domain helper")
        .split("type CommunicationActionRow")
        .next()
        .expect("bounded submitted communication domain helper");
    assert!(historical_domain.contains("submitted_execution_employer_domain"));

    let claim = source
        .split("pub fn claim_communication_action(")
        .nth(1)
        .expect("CommunicationDispatch claim")
        .split("fn validate_communication_success_evidence(")
        .next()
        .expect("bounded CommunicationDispatch claim")
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL CommunicationDispatch claim");
    let assert_ordered = |path: &str, operations: &[&str], label: &str| {
        let mut previous = 0;
        for operation in operations {
            let position = path
                .find(operation)
                .unwrap_or_else(|| panic!("missing {label} operation {operation}"));
            assert!(position >= previous, "{label} order inverted at {operation}");
            previous = position;
        }
    };
    assert_ordered(
        claim,
        &[
            "lock_operational_hold_shared_postgres_tx",
            "lock_managed_cloud_release_registry_shared_postgres_tx",
            "lock_postgres_ats_certification",
        ],
        "communication claim common prelock",
    );
    let expired = claim
        .split("if let Some(expired)")
        .nth(1)
        .expect("expired CommunicationDispatch reclaim")
        .split("let initial_scan_cursor =")
        .next()
        .expect("bounded expired CommunicationDispatch reclaim");
    assert_ordered(
        expired,
        &[
            "lock_discovery_account_shared_postgres",
            "require_active_account_write_fence_postgres_tx",
            "FOR UPDATE",
            "communication_post_lock_db_now_postgres_tx",
            "SET status = 'side_effect_unknown'",
        ],
        "expired communication reclaim",
    );
    let fresh = claim
        .split("let initial_scan_cursor =")
        .nth(1)
        .expect("fresh CommunicationDispatch claim");
    assert_ordered(
        fresh,
        &[
            "lock_discovery_account_shared_postgres",
            "communication_dispatch_is_held_postgres_tx_after_authority_prelock",
            "require_active_account_write_fence_postgres_tx",
            "FOR UPDATE OF a, c, credential",
            "FOR SHARE",
            "communication_post_lock_db_now_postgres_tx",
            "INSERT INTO jobs_communication_action_attempts",
            "SET status = 'dispatching'",
        ],
        "fresh communication claim",
    );

    let request_start = source
        .split("pub fn mark_communication_action_request_started(")
        .nth(1)
        .expect("CommunicationDispatch request-start")
        .split("pub fn finish_communication_action(")
        .next()
        .expect("bounded CommunicationDispatch request-start")
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL CommunicationDispatch request-start");
    let mut previous = 0;
    for operation in [
        "lock_operational_hold_shared_postgres_tx",
        "lock_managed_cloud_release_registry_shared_postgres_tx",
        "lock_postgres_ats_certification",
        "lock_discovery_account_shared_postgres",
        "communication_employer_domain_postgres_tx_after_authority_prelock",
        "SELECT evidence_sha256",
    ] {
        let position = request_start
            .find(operation)
            .unwrap_or_else(|| panic!("missing request-start operation {operation}"));
        assert!(
            position >= previous,
            "request-start order inverted at {operation}"
        );
        previous = position;
    }
    let replay = request_start
        .split("if let Some(existing_evidence_sha256)")
        .nth(1)
        .expect("request-start durable replay")
        .split("let dispatch_held =")
        .next()
        .expect("bounded request-start durable replay");
    let replay_time = replay
        .find("communication_post_lock_db_now_postgres_tx")
        .expect("request-start replay database time");
    let replay_lease = replay
        .find("require_communication_lease_current_at_ms")
        .expect("request-start replay lease validation");
    let replay_exact = replay
        .find("existing_evidence_sha256 != evidence_sha256")
        .expect("request-start replay exact evidence check");
    let replay_commit = replay.find("tx.commit()?").expect("request-start replay commit");
    assert!(replay_time < replay_lease && replay_lease < replay_exact);
    assert!(replay_exact < replay_commit);

    let fresh = request_start
        .split("let dispatch_held =")
        .nth(1)
        .expect("request-start fresh path")
        .split("if inserted == 0")
        .next()
        .expect("bounded request-start fresh path");
    let mut previous = 0;
    for operation in [
        "communication_dispatch_is_held_postgres_tx_with_domain_after_authority_prelock",
        "communication_post_lock_db_now_postgres_tx",
        "require_communication_lease_current_at_ms",
        "INSERT INTO jobs_communication_action_attempt_evidence",
    ] {
        let position = fresh
            .find(operation)
            .unwrap_or_else(|| panic!("missing fresh request-start operation {operation}"));
        assert!(
            position >= previous,
            "fresh request-start order inverted at {operation}"
        );
        previous = position;
    }
}

#[cfg(test)]
#[test]
fn fix_753_postgres_communication_lease_paths_sample_time_after_final_lock() {
    let source = include_str!("communication_actions.rs");
    let bounded = |start: &str, end: &str| {
        source
            .split(start)
            .nth(1)
            .unwrap_or_else(|| panic!("missing communication path {start}"))
            .split(end)
            .next()
            .unwrap_or_else(|| panic!("missing communication path boundary {end}"))
    };

    let finish = bounded(
        "pub fn finish_communication_action(",
        "pub fn claim_communication_action_reconciliation(",
    )
    .split("DbPool::Postgres(_) =>")
    .nth(1)
    .expect("PostgreSQL communication finish");
    let finish_action = finish.find("FOR UPDATE").expect("finish action lock");
    let finish_source = finish
        .find("communication_reply_source_postgres_tx")
        .expect("finish reply-source lock");
    let finish_time = finish
        .find("communication_post_lock_db_now_postgres_tx")
        .expect("finish post-lock database time");
    let finish_current = finish
        .find("require_communication_lease_current_at_ms")
        .expect("finish current-lease validation");
    let finish_mutation = finish
        .find("INSERT INTO jobs_communication_action_attempt_evidence")
        .expect("finish evidence mutation");
    assert!(finish_action < finish_source);
    assert!(finish_source < finish_time && finish_time < finish_current);
    assert!(finish_current < finish_mutation);
    assert!(!finish[..finish_time].contains("lease_expires_at_ms > $"));

    let reconcile = bounded("pub fn reconcile_communication_action(", "#[cfg(test)]")
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL communication reconciliation");
    let reconcile_action = reconcile.find("FOR UPDATE").expect("reconcile action lock");
    let reconcile_mailbox = reconcile.find("FOR SHARE").expect("reconcile mailbox lock");
    let reconcile_source = reconcile
        .find("communication_reply_source_postgres_tx")
        .expect("reconcile reply-source lock");
    let reconcile_time = reconcile
        .find("communication_post_lock_db_now_postgres_tx")
        .expect("reconcile post-lock database time");
    let reconcile_current = reconcile
        .find("require_communication_lease_current_at_ms")
        .expect("reconcile current-lease validation");
    let reconcile_mutation = reconcile
        .find("INSERT INTO jobs_communication_action_reconciliations")
        .expect("reconcile evidence mutation");
    assert!(reconcile_action < reconcile_mailbox);
    assert!(reconcile_mailbox < reconcile_source);
    assert!(reconcile_source < reconcile_time && reconcile_time < reconcile_current);
    assert!(reconcile_current < reconcile_mutation);
    assert!(!reconcile[..reconcile_time].contains("lease_expires_at_ms > $"));

    let reconciliation_claim = bounded(
        "pub fn claim_communication_action_reconciliation(",
        "pub fn reconcile_communication_action(",
    )
    .split("DbPool::Postgres(_) =>")
    .nth(1)
    .expect("PostgreSQL communication reconciliation claim");
    let claim_lock = reconciliation_claim
        .find("FOR UPDATE OF a, mailbox")
        .expect("reconciliation action/mailbox lock");
    let claim_time = reconciliation_claim
        .find("communication_post_lock_db_now_postgres_tx")
        .expect("reconciliation-claim post-lock database time");
    let claim_mutation = reconciliation_claim
        .find("UPDATE jobs_communication_actions")
        .expect("reconciliation-claim mutation");
    assert!(claim_lock < claim_time && claim_time < claim_mutation);
    assert!(!reconciliation_claim[claim_lock..claim_time].contains("a.lease_expires_at_ms <= $"));
}

#[cfg(test)]
fn wait_for_communication_postgres_lock_wait(
    tx: &mut postgres::Transaction<'_>,
    application_name: &str,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let waiting: i64 = tx
            .query_one(
                "SELECT COUNT(*)::bigint FROM pg_stat_activity
                  WHERE application_name = $1 AND wait_event_type = 'Lock'",
                &[&application_name],
            )
            .expect("observe communication lock waiter")
            .get(0);
        if waiting == 1 {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(test)]
#[test]
fn fix_753_postgres_communication_contention_uses_post_lock_time() {
    let Ok(url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let schema = format!("fix753_communication_{suffix}");
    let request_waiter_name = format!(
        "bluey-fix753-communication-request-{}",
        &suffix[..16]
    );
    let claim_waiter_name = format!(
        "bluey-fix753-communication-claim-{}",
        &suffix[..16]
    );
    let mut setup = postgres::Client::connect(&url, postgres::NoTls)
        .expect("connect FIX-753 communication PostgreSQL setup client");
    let setup_now_ms: i64 = setup
        .query_one(
            "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
            &[],
        )
        .expect("sample FIX-753 communication setup time")
        .get(0);
    let request_expires_at_ms = setup_now_ms.saturating_add(500);
    setup
        .batch_execute(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.actions (
                 id text PRIMARY KEY,
                 status text NOT NULL,
                 lease_expires_at_ms bigint,
                 updated_at_ms bigint NOT NULL DEFAULT 0
             );
             CREATE TABLE {schema}.evidence (
                 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                 action_id text NOT NULL
             );
             INSERT INTO {schema}.actions(id,status,lease_expires_at_ms)
             VALUES ('request','dispatching',{request_expires_at_ms}),
                    ('claim','approved',NULL);"
        ))
        .expect("create isolated FIX-753 communication fixture");

    let mut request_holder = setup
        .transaction()
        .expect("begin FIX-753 communication request holder");
    request_holder
        .query_one(
            &format!("SELECT id FROM {schema}.actions WHERE id = 'request' FOR UPDATE"),
            &[],
        )
        .expect("lock FIX-753 communication request row");
    let request_url = url.clone();
    let request_schema = schema.clone();
    let request_application_name = request_waiter_name.clone();
    let (request_started_tx, request_started_rx) = std::sync::mpsc::channel();
    let request_waiter = std::thread::spawn(move || -> std::result::Result<bool, String> {
        let mut client = postgres::Client::connect(&request_url, postgres::NoTls)
            .map_err(|error| error.to_string())?;
        client
            .query_one(
                "SELECT set_config('application_name', $1, false)",
                &[&request_application_name],
            )
            .map_err(|error| error.to_string())?;
        let mut tx = client.transaction().map_err(|error| error.to_string())?;
        request_started_tx
            .send(())
            .map_err(|error| error.to_string())?;
        let expires_at_ms: i64 = tx
            .query_one(
                &format!(
                    "SELECT lease_expires_at_ms FROM {request_schema}.actions
                      WHERE id = 'request' FOR UPDATE"
                ),
                &[],
            )
            .map_err(|error| error.to_string())?
            .get(0);
        let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)
            .map_err(|error| error.to_string())?;
        let authorized = expires_at_ms > effect_now_ms;
        if authorized {
            tx.execute(
                &format!("INSERT INTO {request_schema}.evidence(action_id) VALUES ('request')"),
                &[],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(authorized)
    });
    request_started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("FIX-753 communication request waiter started");
    let request_waited =
        wait_for_communication_postgres_lock_wait(&mut request_holder, &request_waiter_name);
    loop {
        let database_now_ms: i64 = request_holder
            .query_one(
                "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                &[],
            )
            .expect("sample held FIX-753 communication request time")
            .get(0);
        if database_now_ms > request_expires_at_ms {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    request_holder
        .commit()
        .expect("release FIX-753 communication request row");
    let request_authorized = request_waiter
        .join()
        .expect("join FIX-753 communication request waiter")
        .expect("complete FIX-753 communication request waiter");
    let request_evidence_count: i64 = setup
        .query_one(
            &format!("SELECT COUNT(*)::bigint FROM {schema}.evidence WHERE action_id = 'request'"),
            &[],
        )
        .expect("count FIX-753 denied communication request evidence")
        .get(0);
    assert!(
        request_waited,
        "communication request did not wait on its exact row"
    );
    assert!(
        !request_authorized,
        "expired communication request was authorized"
    );
    assert_eq!(request_evidence_count, 0, "expired request wrote evidence");

    const TEST_LEASE_TTL_MS: i64 = 250;
    let mut claim_holder = setup
        .transaction()
        .expect("begin FIX-753 communication claim holder");
    claim_holder
        .query_one(
            &format!("SELECT id FROM {schema}.actions WHERE id = 'claim' FOR UPDATE"),
            &[],
        )
        .expect("lock FIX-753 communication claim row");
    let claim_url = url.clone();
    let claim_schema = schema.clone();
    let claim_application_name = claim_waiter_name.clone();
    let (claim_preliminary_tx, claim_preliminary_rx) = std::sync::mpsc::channel();
    let claim_waiter =
        std::thread::spawn(move || -> std::result::Result<(i64, i64, i64), String> {
            let mut client = postgres::Client::connect(&claim_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            client
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&claim_application_name],
                )
                .map_err(|error| error.to_string())?;
            let preliminary_now_ms: i64 = client
                .query_one(
                    "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            claim_preliminary_tx
                .send(preliminary_now_ms)
                .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {claim_schema}.actions WHERE id = 'claim' FOR UPDATE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            let effect_now_ms = communication_post_lock_db_now_postgres_tx(&mut tx)
                .map_err(|error| error.to_string())?;
            let expires_at_ms = effect_now_ms.saturating_add(TEST_LEASE_TTL_MS);
            tx.execute(
                &format!(
                    "UPDATE {claim_schema}.actions
                        SET status = 'dispatching', lease_expires_at_ms = $1,
                            updated_at_ms = $2 WHERE id = 'claim'"
                ),
                &[&expires_at_ms, &effect_now_ms],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            Ok((preliminary_now_ms, effect_now_ms, expires_at_ms))
        });
    let claim_preliminary_now_ms = claim_preliminary_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("FIX-753 communication claim waiter started");
    let claim_waited =
        wait_for_communication_postgres_lock_wait(&mut claim_holder, &claim_waiter_name);
    loop {
        let database_now_ms: i64 = claim_holder
            .query_one(
                "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                &[],
            )
            .expect("sample held FIX-753 communication claim time")
            .get(0);
        if database_now_ms > claim_preliminary_now_ms.saturating_add(TEST_LEASE_TTL_MS) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    claim_holder
        .commit()
        .expect("release FIX-753 communication claim row");
    let (preliminary_now_ms, effect_now_ms, expires_at_ms) = claim_waiter
        .join()
        .expect("join FIX-753 communication claim waiter")
        .expect("complete FIX-753 communication claim waiter");
    setup
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("remove isolated FIX-753 communication fixture");
    assert!(
        claim_waited,
        "communication claim did not wait on its exact row"
    );
    assert!(
        effect_now_ms > preliminary_now_ms.saturating_add(TEST_LEASE_TTL_MS),
        "communication claim reused its expired preliminary lease clock"
    );
    assert_eq!(
        expires_at_ms,
        effect_now_ms.saturating_add(TEST_LEASE_TTL_MS),
        "communication claim did not derive expiry from post-lock time"
    );
}
