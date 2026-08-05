type ExecutionLeaseResult<T> = std::result::Result<T, ExecutionLeaseError>;

#[derive(Debug)]
struct StoredExecutionLease {
    account_id: String,
    application_id: String,
    browser_profile_id: String,
    owner_id: String,
    lease_token_sha256: String,
    fence: i64,
    phase: String,
    lease_expires_at_ms: i64,
}

pub fn execution_browser_profile_id(account_id: &str, identity_id: &str) -> String {
    format!(
        "{}:{}",
        execution_scope_digest(account_id),
        execution_scope_digest(identity_id)
    )
}

fn execution_scope_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))[..24].to_string()
}

fn validate_execution_binding(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.trim() == value
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn validate_execution_access(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || lease_token.is_empty()
        || lease_token.len() > 256
        || fence <= 0
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn validate_final_submit_proof(
    account_id: &str,
    application: &JobApplication,
    proof: &FinalSubmitProof,
) -> ExecutionLeaseResult<()> {
    let approved = approved_submission_snapshot(account_id, application)
        .map_err(|_| ExecutionLeaseError::Conflict)?;
    let canonical_url = approved
        .job
        .get("canonicalUrl")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let host = reqwest::Url::parse(canonical_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let expected_provider = match host.as_str() {
        "boards.greenhouse.io" | "job-boards.greenhouse.io" => (
            "greenhouse",
            "2026.07.1-beta.1",
            "greenhouse_submit_application",
        ),
        "jobs.lever.co" | "jobs.eu.lever.co" => {
            ("lever", "2026.07.0-beta.1", "lever_application_submit")
        }
        _ => return Err(ExecutionLeaseError::Conflict),
    };
    let approved_job_key = final_submit_provider_job_key(expected_provider.0, canonical_url)
        .map_err(|_| ExecutionLeaseError::Conflict)?;
    let page_url = final_submit_provider_url(&proof.job.page_url)?;
    let action_url = final_submit_provider_url(&proof.target.action_url)?;
    let page_job_key = final_submit_provider_job_key(expected_provider.0, &proof.job.page_url)?;
    let action_job_key =
        final_submit_provider_job_key(expected_provider.0, &proof.target.action_url)?;
    if proof.schema_version != 3
        || proof.adapter != expected_provider.0
        || proof.adapter_version != expected_provider.1
        || proof.control != expected_provider.2
        || proof.job.approved_canonical_url != canonical_url
        || page_job_key != approved_job_key
        || action_job_key != approved_job_key
        || proof.target.provider_job_key != approved_job_key
        || proof.target.method != "post"
        || proof.target.enctype != "multipart/form-data"
        || proof.target.form_target != "_self"
        || !same_final_submit_origin(&page_url, &action_url)
        || !validate_execution_binding(&proof.target.form_identity, 1_024)
        || proof.documents.is_empty()
        || proof.documents.len() > 2
        || proof.files.len() != proof.documents.len()
        || proof.fields.is_empty()
        || proof.fields.len() > 256
        || !valid_final_submit_part_order(proof)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let resume_version_id = approved
        .packet
        .get("resumeVersionId")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let cover_letter_required = approved
        .packet
        .get("coverLetterContent")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let mut expected_kinds = if cover_letter_required {
        vec!["cover_letter", "resume"]
    } else {
        vec!["resume"]
    };
    expected_kinds.sort_unstable();
    let actual_kinds = proof
        .documents
        .iter()
        .map(|document| document.kind.as_str())
        .collect::<Vec<_>>();
    if actual_kinds != expected_kinds {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let mut aggregate_field_bytes = 0_i64;
    let file_field_names = proof
        .files
        .iter()
        .map(|file| file.field_name.as_str())
        .collect::<BTreeSet<_>>();
    for field in &proof.fields {
        aggregate_field_bytes = aggregate_field_bytes
            .checked_add(field.value_byte_length)
            .ok_or(ExecutionLeaseError::InvalidRequest)?;
        if !valid_final_submit_field_name(&field.field_name)
            || file_field_names.contains(field.field_name.as_str())
            || field.value_byte_length < 0
            || field.value_byte_length > 65_536
            || !valid_final_submit_sha256(&field.value_sha256)
        {
            return Err(ExecutionLeaseError::InvalidRequest);
        }
    }
    if aggregate_field_bytes > 524_288 {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let mut aggregate_file_bytes = 0_i64;
    for file in &proof.files {
        let Some((kind, name_sha256)) = final_submit_file_kind_and_hash(&file.name) else {
            return Err(ExecutionLeaseError::InvalidRequest);
        };
        aggregate_file_bytes = aggregate_file_bytes
            .checked_add(file.byte_length)
            .ok_or(ExecutionLeaseError::InvalidRequest)?;
        if !valid_final_submit_field_name(&file.field_name)
            || file.byte_length < 1
            || file.byte_length > 12 * 1024 * 1024
            || !valid_final_submit_sha256(&file.sha256)
            || file.sha256 != name_sha256
            || proof
                .documents
                .iter()
                .filter(|document| document.kind == kind && document.sha256 == file.sha256)
                .count()
                != 1
        {
            return Err(ExecutionLeaseError::InvalidRequest);
        }
    }
    if aggregate_file_bytes > 24 * 1024 * 1024 {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    for document in &proof.documents {
        if !valid_final_submit_sha256(&document.sha256)
            || proof
                .files
                .iter()
                .filter(|file| {
                    final_submit_file_kind_and_hash(&file.name).is_some_and(|(kind, _)| {
                        kind == document.kind && file.sha256 == document.sha256
                    })
                })
                .count()
                != 1
            || match document.kind.as_str() {
                "resume" => document.version_id.as_deref() != Some(resume_version_id),
                "cover_letter" => document.version_id.is_some(),
                _ => true,
            }
        {
            return Err(ExecutionLeaseError::InvalidRequest);
        }
    }
    Ok(())
}

fn final_submit_file_kind_and_hash(name: &str) -> Option<(&'static str, &str)> {
    let stem = name.strip_suffix(".pdf")?;
    let (kind, sha256) = if let Some(sha256) = stem.strip_prefix("resume-") {
        ("resume", sha256)
    } else if let Some(sha256) = stem.strip_prefix("cover-letter-") {
        ("cover_letter", sha256)
    } else {
        return None;
    };
    valid_final_submit_sha256(sha256).then_some((kind, sha256))
}

fn valid_final_submit_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_final_submit_field_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'[' | b']')
        })
}

fn valid_final_submit_part_order(proof: &FinalSubmitProof) -> bool {
    if proof.part_order.len() != proof.fields.len().saturating_add(proof.files.len()) {
        return false;
    }
    let mut seen_fields = vec![false; proof.fields.len()];
    let mut seen_files = vec![false; proof.files.len()];
    for part in &proof.part_order {
        let Ok(index) = usize::try_from(part.index) else {
            return false;
        };
        let seen = match part.kind.as_str() {
            "field" => seen_fields.get_mut(index),
            "file" => seen_files.get_mut(index),
            _ => return false,
        };
        let Some(seen) = seen else {
            return false;
        };
        if *seen {
            return false;
        }
        *seen = true;
    }
    seen_fields.into_iter().all(|seen| seen) && seen_files.into_iter().all(|seen| seen)
}

#[derive(Clone, Copy)]
enum FinalSubmitProviderJobKeyPurpose {
    Submit,
    Confirmation,
}

pub(crate) fn final_submit_provider_job_key(
    provider: &str,
    raw_url: &str,
) -> std::result::Result<String, ExecutionLeaseError> {
    final_submit_provider_job_key_for_purpose(
        provider,
        raw_url,
        FinalSubmitProviderJobKeyPurpose::Submit,
    )
}

pub(crate) fn final_submit_confirmation_provider_job_key(
    provider: &str,
    raw_url: &str,
) -> std::result::Result<String, ExecutionLeaseError> {
    final_submit_provider_job_key_for_purpose(
        provider,
        raw_url,
        FinalSubmitProviderJobKeyPurpose::Confirmation,
    )
}

fn final_submit_provider_job_key_for_purpose(
    provider: &str,
    raw_url: &str,
    purpose: FinalSubmitProviderJobKeyPurpose,
) -> ExecutionLeaseResult<String> {
    let url = final_submit_provider_url(raw_url)?;
    match provider {
        "greenhouse" => greenhouse_final_submit_job_key(&url, purpose),
        "lever" => lever_final_submit_job_key(&url, purpose),
        _ => Err(ExecutionLeaseError::InvalidRequest),
    }
}

fn final_submit_provider_url(raw_url: &str) -> ExecutionLeaseResult<reqwest::Url> {
    if !validate_execution_binding(raw_url, 2_048) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let url = reqwest::Url::parse(raw_url).map_err(|_| ExecutionLeaseError::InvalidRequest)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(url)
}

fn same_final_submit_origin(left: &reqwest::Url, right: &reqwest::Url) -> bool {
    left.scheme() == right.scheme()
        && left
            .host_str()
            .map(str::to_ascii_lowercase)
            .eq(&right.host_str().map(str::to_ascii_lowercase))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn greenhouse_final_submit_job_key(
    url: &reqwest::Url,
    purpose: FinalSubmitProviderJobKeyPurpose,
) -> ExecutionLeaseResult<String> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(
        host.as_str(),
        "boards.greenhouse.io" | "job-boards.greenhouse.io"
    ) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let job_indexes = segments
        .iter()
        .enumerate()
        .filter_map(|(index, segment)| segment.eq_ignore_ascii_case("jobs").then_some(index))
        .collect::<Vec<_>>();
    if job_indexes.len() > 1 {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let job_index = job_indexes.first().copied();
    let valid_path = match (purpose, job_index) {
        (FinalSubmitProviderJobKeyPurpose::Submit, Some(1)) => segments.len() == 3,
        (FinalSubmitProviderJobKeyPurpose::Submit, None) => {
            segments.len() == 2
                && segments[0].eq_ignore_ascii_case("embed")
                && segments[1].eq_ignore_ascii_case("job_app")
        }
        (FinalSubmitProviderJobKeyPurpose::Confirmation, Some(1)) => {
            segments.len() == 4 && segments[3].eq_ignore_ascii_case("confirmation")
        }
        _ => false,
    };
    if !valid_path {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let path_tenant = job_index
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| segments.get(index))
        .map(|value| (*value).to_string());
    let path_job = job_index
        .and_then(|index| segments.get(index + 1))
        .map(|value| (*value).to_string());
    let tenant = one_provider_job_identifier(
        routing_query_values(url, &["for"])
            .into_iter()
            .chain(path_tenant),
    )?;
    let job = one_provider_job_identifier(
        routing_query_values(
            url,
            &[
                "gh_jid",
                "token",
                "job_id",
                "jobid",
                "posting_id",
                "postingid",
            ],
        )
        .into_iter()
        .chain(path_job),
    )?;
    Ok(format!("greenhouse:{tenant}:{job}"))
}

fn lever_final_submit_job_key(
    url: &reqwest::Url,
    purpose: FinalSubmitProviderJobKeyPurpose,
) -> ExecutionLeaseResult<String> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co") {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let valid_path = match purpose {
        FinalSubmitProviderJobKeyPurpose::Submit => {
            segments.len() == 2
                || (segments.len() == 3 && segments[2].eq_ignore_ascii_case("apply"))
        }
        FinalSubmitProviderJobKeyPurpose::Confirmation => {
            segments.len() == 3 && segments[2].eq_ignore_ascii_case("confirmation")
        }
    };
    if !valid_path
        || !valid_provider_job_identifier(segments[0])
        || !valid_provider_job_identifier(segments[1])
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let job = one_provider_job_identifier(
        routing_query_values(
            url,
            &["posting_id", "postingid", "job_id", "jobid", "lever_job_id"],
        )
        .into_iter()
        .chain(std::iter::once(segments[1].to_string())),
    )?;
    Ok(format!("lever:{host}:{}:{job}", segments[0]))
}

fn routing_query_values(url: &reqwest::Url, aliases: &[&str]) -> Vec<String> {
    url.query_pairs()
        .filter(|(key, _)| aliases.iter().any(|alias| key.eq_ignore_ascii_case(alias)))
        .map(|(_, value)| value.into_owned())
        .collect()
}

fn one_provider_job_identifier(
    values: impl Iterator<Item = String>,
) -> ExecutionLeaseResult<String> {
    let values = values.collect::<Vec<_>>();
    let Some(first) = values.first() else {
        return Err(ExecutionLeaseError::InvalidRequest);
    };
    if !valid_provider_job_identifier(first)
        || values
            .iter()
            .any(|value| !valid_provider_job_identifier(value) || value != first)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(first.clone())
}

fn valid_provider_job_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn bind_final_submit_proof_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    proof: &FinalSubmitProof,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let (job_id, raw): (String, String) = tx
        .query_row(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let mut application = parse_application_json(raw, application_id, &job_id, "job application")?;
    validate_final_submit_proof(account_id, &application, proof)?;
    bind_final_submit_proof_value(&mut application, proof)?;
    let payload = to_json(&application, "job application")?;
    if tx.execute(
        "UPDATE jobs_applications SET application_json = ?3, updated_at_ms = ?4
          WHERE account_id = ?1 AND id = ?2",
        params![account_id, application_id, payload, now],
    )? != 1
    {
        return Err(ExecutionLeaseError::NotFound);
    }
    Ok(())
}

fn bind_final_submit_proof_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    proof: &FinalSubmitProof,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let job_id: String = row.try_get(0)?;
    let raw: String = row.try_get(1)?;
    let mut application = parse_application_json(raw, application_id, &job_id, "job application")?;
    validate_final_submit_proof(account_id, &application, proof)?;
    bind_final_submit_proof_value(&mut application, proof)?;
    let payload = to_json(&application, "job application")?;
    if tx.execute(
        "UPDATE jobs_applications SET application_json = $3, updated_at_ms = $4
          WHERE account_id = $1 AND id = $2",
        &[&account_id, &application_id, &payload, &now],
    )? != 1
    {
        return Err(ExecutionLeaseError::NotFound);
    }
    Ok(())
}

fn bind_final_submit_proof_value(
    application: &mut JobApplication,
    proof: &FinalSubmitProof,
) -> ExecutionLeaseResult<()> {
    if !application.receipt.is_object() {
        return Err(ExecutionLeaseError::Conflict);
    }
    let proof_value = serde_json::to_value(proof).map_err(anyhow::Error::from)?;
    if let Some(existing) = application.receipt.get(FINAL_SUBMIT_PROOF_KEY) {
        if existing == &proof_value {
            return Ok(());
        }
        return Err(ExecutionLeaseError::Conflict);
    }
    application
        .receipt
        .as_object_mut()
        .expect("receipt checked above")
        .insert(FINAL_SUBMIT_PROOF_KEY.to_string(), proof_value);
    Ok(())
}

pub fn stored_final_submit_proof(application: &JobApplication) -> Result<FinalSubmitProof> {
    let value = if application.state == "submitted" {
        application.receipt.pointer(&format!(
            "/{SERVER_SUBMISSION_AUTHORITY_KEY}/preSubmissionReceipt/{FINAL_SUBMIT_PROOF_KEY}"
        ))
    } else {
        application.receipt.get(FINAL_SUBMIT_PROOF_KEY)
    }
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("final submit proof is missing"))?;
    serde_json::from_value(value).context("parse final submit proof")
}

fn random_execution_lease_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn execution_lease_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn execution_lease_token_matches(stored_hash: &str, token: &str) -> bool {
    let supplied_hash = execution_lease_token_hash(token);
    supplied_hash.len() == stored_hash.len()
        && supplied_hash
            .as_bytes()
            .ct_eq(stored_hash.as_bytes())
            .unwrap_u8()
            == 1
}

/// Verifies a no-op replay of an immutable cloud submission receipt without
/// consulting the execution-lease row. The submitted application already
/// contains the exact terminal lease authority; retaining the opaque token and
/// fence proves continuity even after bounded lease/capacity cleanup.
pub fn submitted_cloud_receipt_replay_authorized(
    application: &JobApplication,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> bool {
    if application.state != "submitted"
        || application.run_id.as_deref() != Some(run_id)
        || application.receipt.get("runner").and_then(Value::as_str) != Some("cloud")
        || application.receipt.get("runId").and_then(Value::as_str) != Some(run_id)
        || !validate_execution_binding(run_id, 240)
        || lease_token.is_empty()
        || lease_token.len() > 256
        || fence <= 0
    {
        return false;
    }
    let Some(execution) = application
        .receipt
        .pointer(&format!(
            "/{SERVER_SUBMISSION_AUTHORITY_KEY}/executionAuthority"
        ))
        .and_then(Value::as_object)
    else {
        return false;
    };
    let owner_id = execution
        .get("ownerId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stored_hash = execution
        .get("leaseTokenSha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    execution.len() == 5
        && execution.get("kind").and_then(Value::as_str) == Some("cloud_execution_lease")
        && validate_execution_binding(owner_id, 240)
        && stored_hash.len() == 64
        && stored_hash == stored_hash.to_ascii_lowercase()
        && stored_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && execution.get("fence").and_then(Value::as_i64) == Some(fence)
        && matches!(
            execution.get("phase").and_then(Value::as_str),
            Some("submitted" | "side_effect_unknown")
        )
        && execution_lease_token_matches(stored_hash, lease_token)
}

fn execution_lease_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredExecutionLease> {
    Ok(StoredExecutionLease {
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        browser_profile_id: row.get(3)?,
        owner_id: row.get(4)?,
        lease_token_sha256: row.get(5)?,
        fence: row.get(6)?,
        phase: row.get(7)?,
        lease_expires_at_ms: row.get(8)?,
    })
}

fn execution_lease_from_pg_row(row: postgres::Row) -> StoredExecutionLease {
    StoredExecutionLease {
        account_id: row.get(1),
        application_id: row.get(2),
        browser_profile_id: row.get(3),
        owner_id: row.get(4),
        lease_token_sha256: row.get(5),
        fence: row.get(6),
        phase: row.get(7),
        lease_expires_at_ms: row.get(8),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_execution_target_payloads(
    application_raw: String,
    application_job_id: &str,
    application_state: &str,
    session_raw: String,
    session_runner: &str,
    identity_raw: String,
    identity_status: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<()> {
    let application = parse_application_json(
        application_raw,
        application_id,
        application_job_id,
        "job application",
    )?;
    if application.run_id.as_deref() != Some(run_id) || application.state != application_state {
        return Err(ExecutionLeaseError::NotFound);
    }
    if !matches!(application_state, "queued" | "running" | "needs_input") {
        return Err(ExecutionLeaseError::Conflict);
    }

    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    if session.id != run_id
        || session.application_id.as_deref() != Some(application_id)
        || session.runner != session_runner
        || session_runner != "cloud"
    {
        return Err(ExecutionLeaseError::NotFound);
    }

    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let frozen_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    if application
        .receipt
        .pointer("/application_identity/verified")
        .and_then(Value::as_bool)
        != Some(true)
        || identity_status != "verified"
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let identity: ApplicationIdentity = parse_json(identity_raw, "application identity")?;
    if identity.id != identity_id || identity.email != frozen_email {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn sqlite_execution_target(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let (application_job_id, application_raw, application_state): (String, String, String) = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let (session_raw, session_runner): (String, String) = tx
        .query_row(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let (identity_raw, identity_status): (String, String) = tx
        .query_row(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::Conflict)?;
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    if !current_execution_authorized_sqlite(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )? {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(execution_browser_profile_id(account_id, identity_id))
}

fn postgres_execution_target(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application_job_id: String = row.get(0);
    let application_raw: String = row.get(1);
    let application_state: String = row.get(2);
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?
        .to_string();
    let row = tx
        .query_opt(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &run_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let session_raw: String = row.get(0);
    let session_runner: String = row.get(1);
    let row = tx
        .query_opt(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?;
    let identity_raw: String = row.get(0);
    let identity_status: String = row.get(1);
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    if !current_execution_authorized_postgres(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )? {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(execution_browser_profile_id(account_id, &identity_id))
}

fn sqlite_next_execution_fence(
    tx: &rusqlite::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx.query_row(
        "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
          WHERE application_id = ?1 OR browser_profile_id = ?2",
        params![application_id, browser_profile_id],
        |row| row.get(0),
    )?;
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

fn postgres_next_execution_fence(
    tx: &mut postgres::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
              WHERE application_id = $1 OR browser_profile_id = $2",
            &[&application_id, &browser_profile_id],
        )?
        .get(0);
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

fn sqlite_bind_cloud_attempt(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let updated = tx.execute(
        "UPDATE jobs_attempt_reservations
            SET runner = 'cloud', updated_at_ms = ?3
          WHERE account_id = ?1 AND application_id = ?2
            AND status IN ('reserved', 'running')
            AND runner IN ('unassigned', 'cloud')",
        params![account_id, application_id, now],
    )?;
    if updated != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn postgres_bind_cloud_attempt(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let updated = tx.execute(
        "UPDATE jobs_attempt_reservations
            SET runner = 'cloud', updated_at_ms = $3
          WHERE account_id = $1 AND application_id = $2
            AND status IN ('reserved', 'running')
            AND runner IN ('unassigned', 'cloud')",
        &[&account_id, &application_id, &now],
    )?;
    if updated != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub fn claim_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
) -> ExecutionLeaseResult<ExecutionLeaseGrant> {
    claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        None,
        None,
    )
    .map(|(lease, _)| lease)
}

#[cfg(debug_assertions)]
pub fn claim_execution_lease_for_runner_volume(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
    volume_binding: &BindRunnerVolumeResidencyRequest,
) -> ExecutionLeaseResult<RunnerVolumeExecutionLeaseGrant> {
    let (lease, binding) = claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        Some(volume_binding),
        None,
    )?;
    let binding = binding.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(RunnerVolumeExecutionLeaseGrant {
        lease,
        purge_subject: binding.purge_subject,
        volume_id: binding.volume_id,
        enrollment_epoch: binding.enrollment_epoch,
        process_instance_id: binding.process_instance_id,
        volume_key_fingerprint: binding.volume_key_fingerprint,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn claim_execution_lease_for_runner_volume_authorized(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
    volume_binding: &BindRunnerVolumeResidencyRequest,
    volume_authority: &VerifiedRunnerVolumeAuthority,
) -> ExecutionLeaseResult<RunnerVolumeExecutionLeaseGrant> {
    let (lease, binding) = claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        Some(volume_binding),
        Some(volume_authority),
    )?;
    let binding = binding.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(RunnerVolumeExecutionLeaseGrant {
        lease,
        purge_subject: binding.purge_subject,
        volume_id: binding.volume_id,
        enrollment_epoch: binding.enrollment_epoch,
        process_instance_id: binding.process_instance_id,
        volume_key_fingerprint: binding.volume_key_fingerprint,
    })
}

#[allow(clippy::too_many_arguments)]
fn claim_execution_lease_inner(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
    volume_binding: Option<&BindRunnerVolumeResidencyRequest>,
    volume_authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> ExecutionLeaseResult<(ExecutionLeaseGrant, Option<RunnerVolumeResidencyBinding>)> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || !validate_execution_binding(supplied_browser_profile_id, 160)
        || !validate_execution_binding(owner_id, 240)
        || volume_binding
            .is_some_and(|binding| binding.account_id != account_id || binding.run_id != run_id)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let lease_token = random_execution_lease_token();
    let lease_token_sha256 = execution_lease_token_hash(&lease_token);
    let now = volume_binding.map_or_else(now_ms, |binding| binding.now_ms);
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
    let proposed_subject = volume_binding.map(|_| new_runner_volume_purge_subject());

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let prepared_binding = match (volume_binding, proposed_subject.as_deref()) {
                (Some(binding), Some(subject)) => Some(
                    prepare_runner_volume_lease_binding_sqlite_tx(&tx, binding, subject)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            if let (Some(binding), Some(authority)) = (volume_binding, volume_authority) {
                consume_runner_volume_authority_sqlite_tx(
                    &tx,
                    authority,
                    "execution_lease_claim",
                    &binding.volume_id,
                    binding.enrollment_epoch,
                    &binding.process_instance_id,
                    binding.now_ms,
                )
                .map_err(execution_lease_from_runner_volume_error)?;
            }
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            sqlite_bind_cloud_attempt(&tx, account_id, application_id, now)?;
            let existing = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?;
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = ?2, lease_token_sha256 = ?3, fence = ?4,
                            lease_expires_at_ms = ?5, updated_at_ms = ?6
                      WHERE run_id = ?1 AND phase = 'prepared' AND fence = ?7",
                    params![
                        run_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                        existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = ?3, finished_at_ms = ?3
                      WHERE run_id <> ?1
                        AND (application_id = ?2 OR browser_profile_id = ?4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= ?3",
                    params![run_id, application_id, now, browser_profile_id],
                )?;
                let active: bool = tx.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_execution_leases
                         WHERE (application_id = ?1 OR browser_profile_id = ?2)
                           AND phase IN ('prepared', 'click_started')
                    )",
                    params![application_id, browser_profile_id],
                    |row| row.get(0),
                )?;
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', ?8, ?9, ?9)",
                    params![
                        run_id,
                        account_id,
                        application_id,
                        browser_profile_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                    ],
                ) {
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            let bound_volume = match (volume_binding, prepared_binding.as_ref()) {
                (Some(binding), Some(prepared)) => Some(
                    finalize_runner_volume_lease_binding_sqlite_tx(&tx, binding, prepared)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            tx.commit()?;
            Ok((
                ExecutionLeaseGrant {
                    run_id: run_id.to_string(),
                    lease_token,
                    fence,
                    lease_expires_at_ms,
                    phase: "prepared".to_string(),
                },
                bound_volume,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let prepared_binding = match (volume_binding, proposed_subject.as_deref()) {
                (Some(binding), Some(subject)) => Some(
                    prepare_runner_volume_lease_binding_postgres_tx(&mut tx, binding, subject)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            if let (Some(binding), Some(authority)) = (volume_binding, volume_authority) {
                consume_runner_volume_authority_postgres_tx(
                    &mut tx,
                    authority,
                    "execution_lease_claim",
                    &binding.volume_id,
                    binding.enrollment_epoch,
                    &binding.process_instance_id,
                    binding.now_ms,
                )
                .map_err(execution_lease_from_runner_volume_error)?;
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            postgres_bind_cloud_attempt(&mut tx, account_id, application_id, now)?;
            let existing = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
                    &[&run_id],
                )?
                .map(execution_lease_from_pg_row);
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = $2, lease_token_sha256 = $3, fence = $4,
                            lease_expires_at_ms = $5, updated_at_ms = $6
                      WHERE run_id = $1 AND phase = 'prepared' AND fence = $7",
                    &[
                        &run_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                        &existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = $3, finished_at_ms = $3
                      WHERE run_id <> $1
                        AND (application_id = $2 OR browser_profile_id = $4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= $3",
                    &[&run_id, &application_id, &now, &browser_profile_id],
                )?;
                let active: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM jobs_execution_leases
                             WHERE (application_id = $1 OR browser_profile_id = $2)
                               AND phase IN ('prepared', 'click_started')
                        )",
                        &[&application_id, &browser_profile_id],
                    )?
                    .get(0);
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'prepared', $8, $9, $9)",
                    &[
                        &run_id,
                        &account_id,
                        &application_id,
                        &browser_profile_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                    ],
                ) {
                    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            let bound_volume = match (volume_binding, prepared_binding.as_ref()) {
                (Some(binding), Some(prepared)) => Some(
                    finalize_runner_volume_lease_binding_postgres_tx(&mut tx, binding, prepared)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            tx.commit()?;
            Ok((
                ExecutionLeaseGrant {
                    run_id: run_id.to_string(),
                    lease_token,
                    fence,
                    lease_expires_at_ms,
                    phase: "prepared".to_string(),
                },
                bound_volume,
            ))
        }
    })
}

fn execution_lease_from_runner_volume_error(error: RunnerVolumePurgeError) -> ExecutionLeaseError {
    match error {
        RunnerVolumePurgeError::InvalidRequest => ExecutionLeaseError::InvalidRequest,
        RunnerVolumePurgeError::NotFound => ExecutionLeaseError::NotFound,
        RunnerVolumePurgeError::Conflict
        | RunnerVolumePurgeError::Unauthorized
        | RunnerVolumePurgeError::NotReady => ExecutionLeaseError::Conflict,
        RunnerVolumePurgeError::Storage(error) => ExecutionLeaseError::Storage(error),
    }
}

fn require_current_runner_volume_binding_sqlite_for_operation(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    #[cfg(debug_assertions)]
    {
        let is_bound: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1)",
            params![run_id],
            |row| row.get(0),
        )?;
        if !is_bound {
            return Ok(());
        }
    }
    require_current_runner_volume_lease_binding_sqlite_tx(tx, account_id, run_id, now)
        .map(|_| ())
        .map_err(execution_lease_from_runner_volume_error)
}

fn require_current_runner_volume_binding_postgres_for_operation(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    #[cfg(debug_assertions)]
    {
        let is_bound: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_execution_lease_volume_bindings WHERE run_id = $1)",
                &[&run_id],
            )?
            .get(0);
        if !is_bound {
            return Ok(());
        }
    }
    require_current_runner_volume_lease_binding_postgres_tx(tx, account_id, run_id, now)
        .map(|_| ())
        .map_err(execution_lease_from_runner_volume_error)
}

fn require_current_runner_volume_identity_sqlite_for_operation(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    #[cfg(debug_assertions)]
    {
        let is_bound: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1)",
            params![run_id],
            |row| row.get(0),
        )?;
        if !is_bound {
            return Ok(());
        }
    }
    require_current_runner_volume_identity_binding_sqlite_tx(tx, account_id, run_id, now)
        .map_err(execution_lease_from_runner_volume_error)
}

fn require_current_runner_volume_identity_postgres_for_operation(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    #[cfg(debug_assertions)]
    {
        let is_bound: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_execution_lease_volume_bindings WHERE run_id = $1)",
                &[&run_id],
            )?
            .get(0);
        if !is_bound {
            return Ok(());
        }
    }
    require_current_runner_volume_identity_binding_postgres_tx(tx, account_id, run_id, now)
        .map_err(execution_lease_from_runner_volume_error)
}

pub fn heartbeat_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = ?6, updated_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8
                    AND lease_expires_at_ms > ?7",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    lease_expires_at_ms,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = $6, updated_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8
                    AND lease_expires_at_ms > $7",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &lease_expires_at_ms,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
    })
}

pub const SUBMISSION_EVIDENCE_PAYLOAD_RESERVED_BYTES: i64 = 40 * 1024 * 1024;
pub const SUBMISSION_RECEIPT_BUNDLE_MAX_RESERVED_BYTES: i64 = 8 * 1024 * 1024;
pub const SUBMISSION_EVIDENCE_RESERVED_OBJECTS: i64 = 13;
const SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS: i64 = i64::MAX;

pub fn submission_evidence_reserved_bytes(max_object_bytes: i64) -> Option<i64> {
    (max_object_bytes > 0).then(|| {
        SUBMISSION_EVIDENCE_PAYLOAD_RESERVED_BYTES
            .saturating_add(max_object_bytes.min(SUBMISSION_RECEIPT_BUNDLE_MAX_RESERVED_BYTES))
    })
}

fn validate_submission_evidence_capacity_binding(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let expected_reserved_bytes =
        submission_evidence_reserved_bytes(capacity.limits.max_object_bytes);
    if capacity.account_id != account_id
        || capacity.application_id != application_id
        || capacity.run_id != run_id
        || capacity.runner != "cloud"
        || expected_reserved_bytes != Some(capacity.reserved_bytes)
        || capacity.reserved_objects != SUBMISSION_EVIDENCE_RESERVED_OBJECTS
        || capacity.now_ms > now
        || now.saturating_sub(capacity.now_ms) > EXECUTION_LEASE_TTL_MS
        || capacity.expires_at_ms
            != capacity
                .now_ms
                .saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_submission_evidence_capacity_outcome_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    previous_phase: &str,
    outcome: &str,
    finished_at_ms: i64,
    now: i64,
    required: bool,
) -> ExecutionLeaseResult<()> {
    let changed = match outcome {
        "submitted" => {
            crate::db::object_uploads::extend_submission_evidence_capacity_expiry_sqlite_tx(
                tx,
                account_id,
                application_id,
                run_id,
                "cloud",
                SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS,
                now,
            )?
        }
        "side_effect_unknown" => {
            let expires_at_ms = finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
            if expires_at_ms <= now {
                false
            } else if previous_phase == "submitted" {
                crate::db::object_uploads::rebind_submission_evidence_capacity_expiry_sqlite_tx(
                    tx,
                    account_id,
                    application_id,
                    run_id,
                    "cloud",
                    expires_at_ms,
                    now,
                )?
            } else {
                crate::db::object_uploads::extend_submission_evidence_capacity_expiry_sqlite_tx(
                    tx,
                    account_id,
                    application_id,
                    run_id,
                    "cloud",
                    expires_at_ms,
                    now,
                )?
            }
        }
        "failed" | "released" => {
            crate::db::object_uploads::release_submission_evidence_capacity_sqlite_tx(
                tx,
                account_id,
                application_id,
                run_id,
                now,
            )?
        }
        _ => return Err(ExecutionLeaseError::InvalidRequest),
    };
    if required && !changed {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_submission_evidence_capacity_outcome_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    previous_phase: &str,
    outcome: &str,
    finished_at_ms: i64,
    now: i64,
    required: bool,
) -> ExecutionLeaseResult<()> {
    let changed = match outcome {
        "submitted" => {
            crate::db::object_uploads::extend_submission_evidence_capacity_expiry_postgres_tx(
                tx,
                account_id,
                application_id,
                run_id,
                "cloud",
                SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS,
                now,
            )?
        }
        "side_effect_unknown" => {
            let expires_at_ms = finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
            if expires_at_ms <= now {
                false
            } else if previous_phase == "submitted" {
                crate::db::object_uploads::rebind_submission_evidence_capacity_expiry_postgres_tx(
                    tx,
                    account_id,
                    application_id,
                    run_id,
                    "cloud",
                    expires_at_ms,
                    now,
                )?
            } else {
                crate::db::object_uploads::extend_submission_evidence_capacity_expiry_postgres_tx(
                    tx,
                    account_id,
                    application_id,
                    run_id,
                    "cloud",
                    expires_at_ms,
                    now,
                )?
            }
        }
        "failed" | "released" => {
            crate::db::object_uploads::release_submission_evidence_capacity_postgres_tx(
                tx,
                account_id,
                application_id,
                run_id,
                now,
            )?
        }
        _ => return Err(ExecutionLeaseError::InvalidRequest),
    };
    if required && !changed {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn start_irreversible_submission(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    validate_submission_evidence_capacity_binding(
        account_id,
        application_id,
        run_id,
        capacity,
        now,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            bind_final_submit_proof_sqlite_tx(
                &tx,
                account_id,
                application_id,
                final_submit_proof,
                now,
            )?;
            crate::db::object_uploads::reserve_submission_evidence_capacity_sqlite_tx(
                &tx, capacity,
            )?;
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = ?6
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5
                    AND phase = 'prepared' AND lease_expires_at_ms > ?6",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            bind_final_submit_proof_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                final_submit_proof,
                now,
            )?;
            crate::db::object_uploads::reserve_submission_evidence_capacity_postgres_tx(
                &mut tx, capacity,
            )?;
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = $6
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5
                    AND phase = 'prepared' AND lease_expires_at_ms > $6",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
    })
}

fn execution_finish_allowed(phase: &str, outcome: &str) -> bool {
    match phase {
        "prepared" => matches!(outcome, "failed" | "released"),
        "click_started" => matches!(outcome, "submitted" | "side_effect_unknown"),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckpointRecoveryOutcome {
    Released,
    SideEffectUnknown,
}

impl CheckpointRecoveryOutcome {
    fn lease_phase(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }

    fn application_state(self) -> &'static str {
        match self {
            Self::Released => "failed",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }

    fn browser_status(self) -> &'static str {
        match self {
            Self::Released => "failed",
            Self::SideEffectUnknown => "needs_input",
        }
    }

    fn browser_step(self) -> &'static str {
        match self {
            Self::Released => "Browser run stopped before submission",
            Self::SideEffectUnknown => "Submission outcome needs reconciliation",
        }
    }

    fn attempt_status(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }
}

struct CheckpointReconciliationPlan {
    outcome: CheckpointRecoveryOutcome,
    application: JobApplication,
    browser_session: BrowserSession,
}

struct CheckpointReconciliationRequest<'a> {
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    owner_id: &'a str,
    lease_token: Option<&'a str>,
    fence: i64,
    checkpoint_version: i64,
    checkpoint_phase: &'a str,
}

fn validate_checkpoint_reconciliation_request(
    request: &CheckpointReconciliationRequest<'_>,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(request.account_id, 240)
        || !validate_execution_binding(request.application_id, 240)
        || !validate_execution_binding(request.run_id, 240)
        || !validate_execution_binding(request.owner_id, 240)
        || request.fence <= 0
        || !matches!(request.checkpoint_version, 1 | 2)
        || !matches!(
            request.checkpoint_phase,
            "prepared"
                | "needs_input"
                | "provider_review"
                | "final_submit_started"
                | "final_submit_activated"
                | "side_effect_unknown"
        )
        || request
            .lease_token
            .is_some_and(|token| token.is_empty() || token.len() > 256)
        || (request.checkpoint_version == 2 && request.lease_token.is_none())
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn checkpoint_recovery_outcome(
    checkpoint_phase: &str,
    lease_phase: &str,
) -> ExecutionLeaseResult<CheckpointRecoveryOutcome> {
    if matches!(
        checkpoint_phase,
        "final_submit_started" | "final_submit_activated" | "side_effect_unknown"
    ) || matches!(
        lease_phase,
        "click_started" | "submitted" | "side_effect_unknown"
    ) {
        return Ok(CheckpointRecoveryOutcome::SideEffectUnknown);
    }
    if matches!(
        checkpoint_phase,
        "prepared" | "needs_input" | "provider_review"
    ) && matches!(lease_phase, "prepared" | "failed" | "released")
    {
        return Ok(CheckpointRecoveryOutcome::Released);
    }
    Err(ExecutionLeaseError::Conflict)
}

fn trusted_submitted_application(application: &JobApplication, evidence_count: i64) -> bool {
    application.state == "submitted"
        && application.submitted_at_ms.is_some_and(|value| value > 0)
        && application
            .receipt
            .get("_bluey_server_submission_fingerprint_v1")
            .and_then(Value::as_str)
            .is_some_and(|fingerprint| {
                fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        && evidence_count > 0
}

fn validate_checkpoint_lease_access(
    lease: &StoredExecutionLease,
    owner_id: &str,
    lease_token: Option<&str>,
    fence: i64,
    checkpoint_version: i64,
) -> ExecutionLeaseResult<()> {
    let token_matches = match (checkpoint_version, lease_token) {
        (1, None) => true,
        (_, Some(token)) => execution_lease_token_matches(&lease.lease_token_sha256, token),
        _ => false,
    };
    if lease.owner_id != owner_id || lease.fence != fence || !token_matches {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn exact_checkpoint_recovery_replay(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease: &StoredExecutionLease,
    checkpoint_version: i64,
    checkpoint_phase: &str,
    application_job_id: &str,
    application_raw: &str,
    application_state: &str,
    session_raw: &str,
    session_runner: &str,
    session_status: &str,
    attempt: Option<&AttemptReservation>,
) -> ExecutionLeaseResult<Option<CheckpointRecoveryOutcome>> {
    let application = parse_application_json(
        application_raw.to_string(),
        application_id,
        application_job_id,
        "job application",
    )?;
    let Some(recovery) = application.receipt.get("cloud_recovery") else {
        return Ok(None);
    };
    let outcome = match recovery.get("status").and_then(Value::as_str) {
        Some("released") => CheckpointRecoveryOutcome::Released,
        Some("side_effect_unknown") => CheckpointRecoveryOutcome::SideEffectUnknown,
        _ => return Err(ExecutionLeaseError::Conflict),
    };
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let browser_session: BrowserSession = parse_json(session_raw.to_string(), "browser session")?;
    if recovery.get("schema_version").and_then(Value::as_i64) != Some(1)
        || recovery.get("run_id").and_then(Value::as_str) != Some(run_id)
        || recovery.get("checkpoint_version").and_then(Value::as_i64) != Some(checkpoint_version)
        || recovery.get("checkpoint_phase").and_then(Value::as_str) != Some(checkpoint_phase)
        || application.id != application_id
        || application.state != application_state
        || application.state != outcome.application_state()
        || application.run_id.as_deref() != Some(run_id)
        || application.submitted_at_ms.is_some()
        || lease.account_id != account_id
        || lease.application_id != application_id
        || lease.phase != outcome.lease_phase()
        || lease.browser_profile_id != execution_browser_profile_id(account_id, identity_id)
        || browser_session.id != run_id
        || browser_session.application_id.as_deref() != Some(application_id)
        || browser_session.runner != session_runner
        || session_runner != "cloud"
        || browser_session.status != session_status
        || session_status != outcome.browser_status()
        || browser_session.current_step != outcome.browser_step()
        || browser_session.takeover_url.is_some()
        || attempt.is_some_and(|attempt| {
            attempt.application_id != application_id
                || attempt.runner != "cloud"
                || attempt.status != outcome.attempt_status()
        })
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(Some(outcome))
}

fn validate_trusted_submitted_checkpoint(
    request: &CheckpointReconciliationRequest<'_>,
    lease: &StoredExecutionLease,
    application_job_id: &str,
    application_raw: String,
    application_state: &str,
    evidence_count: i64,
) -> ExecutionLeaseResult<bool> {
    if application_state != "submitted" {
        return Ok(false);
    }
    let application = parse_application_json(
        application_raw,
        request.application_id,
        application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    if application.run_id.as_deref() != Some(request.run_id)
        || lease.browser_profile_id != execution_browser_profile_id(request.account_id, identity_id)
        || !trusted_submitted_application(&application, evidence_count)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn checkpoint_reconciliation_plan(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease: &StoredExecutionLease,
    checkpoint_version: i64,
    checkpoint_phase: &str,
    application_job_id: &str,
    application_raw: String,
    application_state: &str,
    session_raw: String,
    session_runner: &str,
    session_status: &str,
    attempt: Option<&AttemptReservation>,
    now: i64,
) -> ExecutionLeaseResult<CheckpointReconciliationPlan> {
    if lease.account_id != account_id || lease.application_id != application_id {
        return Err(ExecutionLeaseError::NotFound);
    }
    let mut application = parse_application_json(
        application_raw,
        application_id,
        application_job_id,
        "job application",
    )?;
    if application.id != application_id
        || application.state != application_state
        || application.run_id.as_deref() != Some(run_id)
    {
        return Err(ExecutionLeaseError::NotFound);
    }
    if !matches!(
        application_state,
        "queued" | "running" | "needs_input" | "failed" | "side_effect_unknown"
    ) {
        return Err(ExecutionLeaseError::Conflict);
    }

    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    if lease.browser_profile_id != execution_browser_profile_id(account_id, identity_id) {
        return Err(ExecutionLeaseError::Conflict);
    }

    let mut browser_session: BrowserSession = parse_json(session_raw, "browser session")?;
    if browser_session.id != run_id
        || browser_session.application_id.as_deref() != Some(application_id)
        || browser_session.runner != session_runner
        || session_runner != "cloud"
        || browser_session.status != session_status
        || !matches!(
            session_status,
            "queued" | "running" | "needs_input" | "failed" | "complete"
        )
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    if let Some(attempt) = attempt {
        if attempt.application_id != application_id
            || attempt.runner != "cloud"
            || !matches!(
                attempt.status.as_str(),
                "reserved" | "running" | "released" | "side_effect_unknown" | "submitted"
            )
        {
            return Err(ExecutionLeaseError::Conflict);
        }
    }

    let outcome = checkpoint_recovery_outcome(checkpoint_phase, &lease.phase)?;
    if !application.receipt.is_object() {
        application.receipt = json!({});
    }
    let recovery = application
        .receipt
        .get("cloud_recovery")
        .filter(|value| {
            value.get("run_id").and_then(Value::as_str) == Some(run_id)
                && value.get("status").and_then(Value::as_str) == Some(outcome.lease_phase())
        })
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "status": outcome.lease_phase(),
                "checkpoint_version": checkpoint_version,
                "checkpoint_phase": checkpoint_phase,
                "lease_phase": lease.phase,
                "recorded_at_ms": now,
                "run_id": run_id,
            })
        });
    application
        .receipt
        .as_object_mut()
        .expect("receipt normalized above")
        .insert("cloud_recovery".to_string(), recovery);
    application.state = outcome.application_state().to_string();
    application.updated_at_ms = now;
    application.submitted_at_ms = None;

    browser_session.status = outcome.browser_status().to_string();
    browser_session.current_step = outcome.browser_step().to_string();
    browser_session.takeover_url = None;
    browser_session.updated_at_ms = now;

    Ok(CheckpointReconciliationPlan {
        outcome,
        application,
        browser_session,
    })
}

pub fn execution_lease_phase_for_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<String>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                params![run_id, account_id, application_id],
                |row| row.get(0),
            )
            .optional()
            .context("get execution lease phase"),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_opt(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3",
                &[&run_id, &account_id, &application_id],
            )?
            .map(|row| row.get(0))),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionReceiptAuthority {
    pub owner_id: String,
    pub lease_token_sha256: String,
    pub fence: i64,
    pub phase: String,
}

#[allow(clippy::too_many_arguments)]
fn validate_execution_receipt_authority(
    lease: StoredExecutionLease,
    finished_at_ms: Option<i64>,
    capacity_runner: Option<String>,
    capacity_state: Option<String>,
    capacity_expires_at_ms: Option<i64>,
    lease_token: &str,
    fence: i64,
    now: i64,
) -> ExecutionLeaseResult<ExecutionReceiptAuthority> {
    let capacity_authorized = match (lease.phase.as_str(), finished_at_ms) {
        ("submitted", Some(_)) => {
            capacity_runner.as_deref() == Some("cloud")
                && ((capacity_state.as_deref() == Some("active")
                    && capacity_expires_at_ms
                        == Some(SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS))
                    || capacity_state.as_deref() == Some("committed"))
        }
        ("side_effect_unknown", Some(finished_at_ms)) => {
            let authority_expires_at_ms =
                finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
            capacity_runner.as_deref() == Some("cloud")
                && capacity_state.as_deref() == Some("active")
                && capacity_expires_at_ms == Some(authority_expires_at_ms)
                && authority_expires_at_ms > now
        }
        _ => false,
    };
    if lease.fence != fence
        || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
        || !matches!(lease.phase.as_str(), "submitted" | "side_effect_unknown")
        || finished_at_ms.is_none()
        || !capacity_authorized
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(ExecutionReceiptAuthority {
        owner_id: lease.owner_id,
        lease_token_sha256: lease.lease_token_sha256,
        fence: lease.fence,
        phase: lease.phase,
    })
}

pub fn execution_receipt_authority(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionReceiptAuthority> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_identity_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            let (lease, finished_at_ms, capacity_runner, capacity_state, capacity_expires_at_ms) =
                tx.query_row(
                    "SELECT l.run_id, l.account_id, l.application_id, l.browser_profile_id,
                            l.owner_id, l.lease_token_sha256, l.fence, l.phase,
                            l.lease_expires_at_ms, l.finished_at_ms,
                            c.runner, c.state, c.expires_at_ms
                       FROM jobs_execution_leases l
                       LEFT JOIN jobs_submission_evidence_capacity c
                         ON c.account_id = l.account_id
                        AND c.application_id = l.application_id
                        AND c.run_id = l.run_id
                      WHERE l.run_id = ?1 AND l.account_id = ?2 AND l.application_id = ?3",
                    params![run_id, account_id, application_id],
                    |row| {
                        Ok((
                            execution_lease_from_sqlite_row(row)?,
                            row.get::<_, Option<i64>>(9)?,
                            row.get::<_, Option<String>>(10)?,
                            row.get::<_, Option<String>>(11)?,
                            row.get::<_, Option<i64>>(12)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let authority = validate_execution_receipt_authority(
                lease,
                finished_at_ms,
                capacity_runner,
                capacity_state,
                capacity_expires_at_ms,
                lease_token,
                fence,
                now,
            )?;
            tx.commit()?;
            Ok(authority)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            let lease_row = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms,
                            finished_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let finished_at_ms = lease_row.get::<_, Option<i64>>(9);
            let lease = execution_lease_from_pg_row(lease_row);
            let capacity = tx.query_opt(
                "SELECT runner, state, expires_at_ms
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                  FOR UPDATE",
                &[&account_id, &application_id, &run_id],
            )?;
            let (capacity_runner, capacity_state, capacity_expires_at_ms) = capacity.map_or(
                (None, None, None),
                |row| (Some(row.get(0)), Some(row.get(1)), Some(row.get(2))),
            );
            let authority = validate_execution_receipt_authority(
                lease,
                finished_at_ms,
                capacity_runner,
                capacity_state,
                capacity_expires_at_ms,
                lease_token,
                fence,
                now,
            )?;
            tx.commit()?;
            Ok(authority)
        }
    })
}

pub fn finish_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    outcome: &str,
) -> ExecutionLeaseResult<()> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    if !matches!(
        outcome,
        "submitted" | "failed" | "side_effect_unknown" | "released"
    ) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_identity_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let (lease, finished_at_ms) = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms,
                            finished_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    |row| {
                        Ok((
                            execution_lease_from_sqlite_row(row)?,
                            row.get::<_, Option<i64>>(9)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                if let Some(finished_at_ms) = finished_at_ms {
                    apply_submission_evidence_capacity_outcome_sqlite_tx(
                        &tx,
                        account_id,
                        application_id,
                        run_id,
                        &lease.phase,
                        outcome,
                        finished_at_ms,
                        now,
                        false,
                    )?;
                }
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = ?6, updated_at_ms = ?7,
                        finished_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    outcome,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            apply_submission_evidence_capacity_outcome_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                &lease.phase,
                outcome,
                now,
                now,
                matches!(outcome, "submitted" | "side_effect_unknown"),
            )?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let (lease, finished_at_ms) = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms,
                            finished_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(|row| {
                    let finished_at_ms = row.get::<_, Option<i64>>(9);
                    (execution_lease_from_pg_row(row), finished_at_ms)
                })
                .ok_or(ExecutionLeaseError::NotFound)?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                if let Some(finished_at_ms) = finished_at_ms {
                    apply_submission_evidence_capacity_outcome_postgres_tx(
                        &mut tx,
                        account_id,
                        application_id,
                        run_id,
                        &lease.phase,
                        outcome,
                        finished_at_ms,
                        now,
                        false,
                    )?;
                }
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = $6, updated_at_ms = $7,
                        finished_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &outcome,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            apply_submission_evidence_capacity_outcome_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                &lease.phase,
                outcome,
                now,
                now,
                matches!(outcome, "submitted" | "side_effect_unknown"),
            )?;
            tx.commit()?;
            Ok(())
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn reconcile_execution_lease_checkpoint(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    owner_id: &str,
    lease_token: Option<&str>,
    fence: i64,
    checkpoint_version: i64,
    checkpoint_phase: &str,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    let request = CheckpointReconciliationRequest {
        account_id,
        application_id,
        run_id,
        owner_id,
        lease_token,
        fence,
        checkpoint_version,
        checkpoint_phase,
    };
    validate_checkpoint_reconciliation_request(&request)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_identity_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_checkpoint_lease_access(
                &lease,
                owner_id,
                lease_token,
                fence,
                checkpoint_version,
            )?;
            let (application_job_id, application_raw, application_state): (String, String, String) =
                tx.query_row(
                    "SELECT job_id, application_json, state FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let evidence_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_application_evidence
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id],
                |row| row.get(0),
            )?;
            if validate_trusted_submitted_checkpoint(
                &request,
                &lease,
                &application_job_id,
                application_raw.clone(),
                &application_state,
                evidence_count,
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: "submitted".to_string(),
                });
            }
            let (session_raw, session_runner, session_status): (String, String, String) = tx
                .query_row(
                    "SELECT session_json, runner, status FROM jobs_browser_sessions
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, run_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let attempt = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok(AttemptReservation {
                            id: row.get(0)?,
                            application_id: row.get(1)?,
                            company_key: row.get(2)?,
                            period_key: row.get(3)?,
                            runner: row.get(4)?,
                            status: row.get(5)?,
                            reserved_at_ms: row.get(6)?,
                            updated_at_ms: row.get(7)?,
                        })
                    },
                )
                .optional()?;
            if let Some(outcome) = exact_checkpoint_recovery_replay(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                &application_raw,
                &application_state,
                &session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
            )? {
                if outcome == CheckpointRecoveryOutcome::Released {
                    apply_submission_evidence_capacity_outcome_sqlite_tx(
                        &tx,
                        account_id,
                        application_id,
                        run_id,
                        &lease.phase,
                        outcome.lease_phase(),
                        now,
                        now,
                        false,
                    )?;
                }
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: outcome.lease_phase().to_string(),
                });
            }
            let plan = checkpoint_reconciliation_plan(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                application_raw,
                &application_state,
                session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
                now,
            )?;
            let application_payload = to_json(&plan.application, "Jobs application")?;
            let session_payload = to_json(&plan.browser_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = ?3, application_json = ?4,
                        updated_at_ms = ?5, submitted_at_ms = NULL
                  WHERE account_id = ?1 AND id = ?2 AND state = ?6",
                params![
                    account_id,
                    application_id,
                    plan.outcome.application_state(),
                    application_payload,
                    now,
                    application_state,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = ?3, session_json = ?4,
                        updated_at_ms = ?5
                  WHERE account_id = ?1 AND id = ?2 AND status = ?6",
                params![
                    account_id,
                    run_id,
                    plan.outcome.browser_status(),
                    session_payload,
                    now,
                    session_status,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if let Some(attempt) = &attempt {
                if tx.execute(
                    "UPDATE jobs_attempt_reservations SET status = ?4, updated_at_ms = ?5
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = ?6",
                    params![
                        attempt.id,
                        account_id,
                        application_id,
                        plan.outcome.attempt_status(),
                        now,
                        attempt.status,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = ?6, updated_at_ms = ?7,
                        finished_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND owner_id = ?4 AND fence = ?5 AND phase = ?8",
                params![
                    run_id,
                    account_id,
                    application_id,
                    owner_id,
                    fence,
                    plan.outcome.lease_phase(),
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            apply_submission_evidence_capacity_outcome_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                &lease.phase,
                plan.outcome.lease_phase(),
                now,
                now,
                plan.outcome == CheckpointRecoveryOutcome::SideEffectUnknown
                    && matches!(lease.phase.as_str(), "click_started" | "submitted"),
            )?;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms: lease.lease_expires_at_ms,
                phase: plan.outcome.lease_phase().to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_checkpoint_lease_access(
                &lease,
                owner_id,
                lease_token,
                fence,
                checkpoint_version,
            )?;
            let application_row = tx
                .query_opt(
                    "SELECT job_id, application_json, state FROM jobs_applications
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let application_job_id: String = application_row.get(0);
            let application_raw: String = application_row.get(1);
            let application_state: String = application_row.get(2);
            let evidence_count: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_application_evidence
                      WHERE account_id = $1 AND application_id = $2",
                    &[&account_id, &application_id],
                )?
                .get(0);
            if validate_trusted_submitted_checkpoint(
                &request,
                &lease,
                &application_job_id,
                application_raw.clone(),
                &application_state,
                evidence_count,
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: "submitted".to_string(),
                });
            }
            let session_row = tx
                .query_opt(
                    "SELECT session_json, runner, status FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let session_raw: String = session_row.get(0);
            let session_runner: String = session_row.get(1);
            let session_status: String = session_row.get(2);
            let attempt = tx
                .query_opt(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .map(|row| AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                });
            if let Some(outcome) = exact_checkpoint_recovery_replay(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                &application_raw,
                &application_state,
                &session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
            )? {
                if outcome == CheckpointRecoveryOutcome::Released {
                    apply_submission_evidence_capacity_outcome_postgres_tx(
                        &mut tx,
                        account_id,
                        application_id,
                        run_id,
                        &lease.phase,
                        outcome.lease_phase(),
                        now,
                        now,
                        false,
                    )?;
                }
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: outcome.lease_phase().to_string(),
                });
            }
            let plan = checkpoint_reconciliation_plan(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                application_raw,
                &application_state,
                session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
                now,
            )?;
            let application_payload = to_json(&plan.application, "Jobs application")?;
            let session_payload = to_json(&plan.browser_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = $3, application_json = $4,
                        updated_at_ms = $5, submitted_at_ms = NULL
                  WHERE account_id = $1 AND id = $2 AND state = $6",
                &[
                    &account_id,
                    &application_id,
                    &plan.outcome.application_state(),
                    &application_payload,
                    &now,
                    &application_state,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = $3, session_json = $4,
                        updated_at_ms = $5
                  WHERE account_id = $1 AND id = $2 AND status = $6",
                &[
                    &account_id,
                    &run_id,
                    &plan.outcome.browser_status(),
                    &session_payload,
                    &now,
                    &session_status,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if let Some(attempt) = &attempt {
                if tx.execute(
                    "UPDATE jobs_attempt_reservations SET status = $4, updated_at_ms = $5
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = $6",
                    &[
                        &attempt.id,
                        &account_id,
                        &application_id,
                        &plan.outcome.attempt_status(),
                        &now,
                        &attempt.status,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = $6, updated_at_ms = $7,
                        finished_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND owner_id = $4 AND fence = $5 AND phase = $8",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &owner_id,
                    &fence,
                    &plan.outcome.lease_phase(),
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            apply_submission_evidence_capacity_outcome_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                &lease.phase,
                plan.outcome.lease_phase(),
                now,
                now,
                plan.outcome == CheckpointRecoveryOutcome::SideEffectUnknown
                    && matches!(lease.phase.as_str(), "click_started" | "submitted"),
            )?;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms: lease.lease_expires_at_ms,
                phase: plan.outcome.lease_phase().to_string(),
            })
        }
    })
}
