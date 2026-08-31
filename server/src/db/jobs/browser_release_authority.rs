const BROWSER_BUILD_AUDIENCE: &str = "bluey-jobs-browser-build-v1";
const BROWSER_APP_ID: &str = "sh.bluey.jobs.browser";
const BROWSER_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const BROWSER_MAX_DESCRIPTOR_BYTES: usize = 4_096;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserBuildProof {
    pub descriptor: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBrowserBuildDescriptor {
    pub release_id: String,
    pub build_id: String,
    pub app_version: String,
    pub app_id: String,
    pub protocol_version: i64,
    pub source_commit: String,
    pub platform: String,
    pub architecture: String,
    pub electron_version: String,
    pub playwright_version: String,
    pub chromium_revision: String,
    pub issued_at_ms: i64,
    pub signing_key_id: String,
    pub descriptor_base64url: String,
    pub signature_base64url: String,
    pub descriptor_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserReleaseClaimBinding {
    pub assignment_sha256: String,
    pub assignment_generation: i64,
    pub channel: String,
    pub channel_head_revision: i64,
    pub channel_transition_sha256: String,
    pub activation_sha256: String,
    pub activation_generation: i64,
    pub trust_generation: i64,
    pub channel_sequence: i64,
    pub trust_policy_sha256: String,
    pub manifest_signature_set_sha256: String,
    pub activation_authorization_signature_set_sha256: String,
    pub manifest_sha256: String,
    pub release_sequence: i64,
    pub artifact_id: String,
    pub artifact_sha256: String,
    pub artifact_url: String,
    pub artifact_filename: String,
    pub artifact_size_bytes: i64,
    pub package_kind: String,
    pub release_id: String,
    pub build_id: String,
    pub app_version: String,
    pub protocol_version: i64,
    pub platform: String,
    pub architecture: String,
    pub build_descriptor_sha256: String,
    pub automation_bundle_sha256: String,
    pub chromium_executable_sha256: String,
    pub published_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct BrowserLocalRunClaimSuccess {
    pub ticket: LocalRunTicket,
    pub response: Value,
    pub response_json: String,
    pub release: BrowserReleaseClaimBinding,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum BrowserLocalRunClaimDisposition {
    Success(Box<BrowserLocalRunClaimSuccess>),
    Rejected,
    DistributionUnavailable,
    ReleaseUnavailable,
    ConflictingReplay,
}

#[derive(Debug, Clone)]
struct BrowserReleaseActivationRow {
    channel_head_revision: i64,
    channel_transition_sha256: String,
    activation_sha256: String,
    activation_generation: i64,
    trust_generation: i64,
    channel_sequence: i64,
    trust_policy_sha256: String,
    canonical_policy_base64url: String,
    manifest_signature_set_sha256: String,
    authorization_signature_set_sha256: String,
    manifest_sha256: String,
    accepted_server_release_ids_json: String,
    expires_at_ms: i64,
}

#[derive(Debug, Clone)]
struct BrowserReleaseManifestRow {
    manifest_id: String,
    release_id: String,
    release_sequence: i64,
    authorization_signature_set_sha256: String,
    published_at_ms: i64,
    artifact_count: i64,
}

#[derive(Debug, Clone)]
struct BrowserReleaseArtifactRow {
    artifact_id: String,
    artifact_sha256: String,
    artifact_url: String,
    artifact_filename: String,
    artifact_size_bytes: i64,
    package_kind: String,
    automation_bundle_sha256: String,
    chromium_executable_sha256: String,
}

type BrowserReleaseArtifactContractRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

#[derive(Debug, Clone)]
struct BrowserPortalArtifactRow {
    artifact_id: String,
    platform: String,
    architecture: String,
    package_kind: String,
    build_descriptor_sha256: String,
    build_descriptor_signing_key_id: String,
    artifact_url: String,
    artifact_filename: String,
    artifact_size_bytes: i64,
    artifact_sha256: String,
    app_content_sha256: String,
    automation_bundle_sha256: String,
    chromium_executable_sha256: String,
}

#[derive(Debug, Clone)]
struct BrowserPortalManifestRow {
    manifest_id: String,
    release_id: String,
    release_sequence: i64,
    build_id: String,
    app_version: String,
    protocol_version: i64,
    published_at_ms: i64,
    artifact_count: i64,
    authorization_signature_set_sha256: String,
}

#[derive(Debug, Clone)]
struct BrowserPortalActivationRow {
    manifest_sha256: String,
    accepted_server_release_ids_json: String,
    expires_at_ms: i64,
    manifest_signature_set_sha256: String,
    activation_authorization_signature_set_sha256: String,
    policy_authorization_signature_set_sha256: String,
    policy_sha256: String,
    policy_trust_generation: i64,
    canonical_policy_base64url: String,
}

#[derive(Debug, Clone)]
pub struct BrowserBuildVerifyingKeyRing {
    keys: BTreeMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BrowserReleaseAuthorityError {
    #[error("invalid Browser build proof")]
    InvalidBuildProof,
    #[error("unknown Browser build signing key")]
    UnknownBuildSigningKey,
    #[error("invalid Browser build signature")]
    InvalidBuildSignature,
}

impl BrowserBuildVerifyingKeyRing {
    pub fn new(keys: BTreeMap<String, String>) -> Result<Self, BrowserReleaseAuthorityError> {
        if keys.is_empty() || keys.len() > 16 {
            return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
        }
        for (key_id, encoded_key) in &keys {
            if !browser_release_safe_id(key_id)
                || browser_release_decode_base64url_exact(encoded_key, 32).is_err()
            {
                return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
            }
        }
        Ok(Self { keys })
    }
}

pub fn validate_browser_release_runtime_config() -> Result<()> {
    browser_release_root_trust_anchor_from_environment().map_err(|error| anyhow::anyhow!(error))?;
    let server_release_id = std::env::var("BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID")
        .context("BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID is required")?;
    if !browser_release_safe_id(&server_release_id) {
        anyhow::bail!("BLUEY_JOBS_BROWSER_SERVER_RELEASE_ID is invalid")
    }
    Ok(())
}

pub fn verify_browser_build_proof(
    proof: &BrowserBuildProof,
    key_ring: &BrowserBuildVerifyingKeyRing,
) -> Result<VerifiedBrowserBuildDescriptor, BrowserReleaseAuthorityError> {
    let descriptor = parse_browser_build_proof_for_claim(proof)?;
    let descriptor_bytes = browser_release_decode_base64url_bounded(
        &descriptor.descriptor_base64url,
        BROWSER_MAX_DESCRIPTOR_BYTES,
    )?;
    let signature = browser_release_decode_base64url_exact(&descriptor.signature_base64url, 64)?;
    let encoded_key = key_ring
        .keys
        .get(&descriptor.signing_key_id)
        .ok_or(BrowserReleaseAuthorityError::UnknownBuildSigningKey)?;
    let public_key = browser_release_decode_base64url_exact(encoded_key, 32)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    use ed25519_dalek::Verifier as _;
    verifying_key
        .verify(
            &descriptor_bytes,
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildSignature)?;

    Ok(descriptor)
}

pub fn parse_browser_build_proof_for_claim(
    proof: &BrowserBuildProof,
) -> Result<VerifiedBrowserBuildDescriptor, BrowserReleaseAuthorityError> {
    let descriptor_bytes =
        browser_release_decode_base64url_bounded(&proof.descriptor, BROWSER_MAX_DESCRIPTOR_BYTES)?;
    browser_release_decode_base64url_exact(&proof.signature, 64)?;
    let parsed = parse_browser_build_descriptor_bytes(&descriptor_bytes)?;

    let mut digest = Sha256::new();
    digest.update(&descriptor_bytes);
    digest.update(b"signature=");
    digest.update(proof.signature.as_bytes());
    digest.update(b"\n");
    Ok(VerifiedBrowserBuildDescriptor {
        release_id: parsed.release_id,
        build_id: parsed.build_id,
        app_version: parsed.app_version,
        app_id: parsed.app_id,
        protocol_version: parsed.protocol_version,
        source_commit: parsed.source_commit,
        platform: parsed.platform,
        architecture: parsed.architecture,
        electron_version: parsed.electron_version,
        playwright_version: parsed.playwright_version,
        chromium_revision: parsed.chromium_revision,
        issued_at_ms: parsed.issued_at_ms,
        signing_key_id: parsed.signing_key_id,
        descriptor_base64url: proof.descriptor.clone(),
        signature_base64url: proof.signature.clone(),
        descriptor_sha256: hex::encode(digest.finalize()),
    })
}

pub(crate) fn claim_local_run_with_browser_release_for_distribution<F>(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    issue_response: F,
) -> Result<BrowserLocalRunClaimDisposition>
where
    F: Fn(&LocalRunTicket, &BrowserReleaseClaimBinding) -> Result<Value>,
{
    claim_local_run_with_browser_release_inner(
        pool,
        run_id,
        ticket_hash,
        claim_nonce,
        descriptor,
        server_release_id,
        true,
        issue_response,
    )
}

#[cfg(test)]
pub fn claim_local_run_with_browser_release<F>(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    issue_response: F,
) -> Result<BrowserLocalRunClaimDisposition>
where
    F: Fn(&LocalRunTicket, &BrowserReleaseClaimBinding) -> Result<Value>,
{
    claim_local_run_with_browser_release_inner(
        pool,
        run_id,
        ticket_hash,
        claim_nonce,
        descriptor,
        server_release_id,
        false,
        issue_response,
    )
}

#[allow(clippy::too_many_arguments)]
fn claim_local_run_with_browser_release_inner<F>(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    require_distribution_ready: bool,
    issue_response: F,
) -> Result<BrowserLocalRunClaimDisposition>
where
    F: Fn(&LocalRunTicket, &BrowserReleaseClaimBinding) -> Result<Value>,
{
    if run_id.trim().is_empty()
        || run_id.len() > 200
        || ticket_hash.trim().is_empty()
        || ticket_hash.len() > 256
        || claim_nonce.len() != 64
        || claim_nonce != claim_nonce.to_ascii_lowercase()
        || !claim_nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !browser_release_safe_id(server_release_id)
    {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    }
    let claim_nonce_sha256 = hex::encode(Sha256::digest(claim_nonce.as_bytes()));
    let claim_request_sha256 =
        browser_claim_request_sha256(run_id, ticket_hash, claim_nonce, descriptor);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let disposition = sqlite_claim_local_run_with_browser_release(
                &transaction,
                run_id,
                ticket_hash,
                &claim_nonce_sha256,
                &claim_request_sha256,
                descriptor,
                server_release_id,
                require_distribution_ready,
                &issue_response,
            )?;
            transaction.commit()?;
            Ok(disposition)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut transaction)
                .map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut transaction)?;
            lock_postgres_ats_certification(&mut transaction)?;
            let account_id = transaction
                .query_opt(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| row.get::<_, String>(0));
            let Some(account_id) = account_id else {
                transaction.commit()?;
                return Ok(BrowserLocalRunClaimDisposition::Rejected);
            };
            lock_discovery_account_shared_postgres(&mut transaction, &account_id)?;
            let disposition = postgres_claim_local_run_with_browser_release(
                &mut transaction,
                &account_id,
                run_id,
                ticket_hash,
                &claim_nonce_sha256,
                &claim_request_sha256,
                descriptor,
                server_release_id,
                require_distribution_ready,
                &issue_response,
            )?;
            transaction.commit()?;
            Ok(disposition)
        }
    })
}

fn browser_claim_request_sha256(
    run_id: &str,
    ticket_hash: &str,
    claim_nonce: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-browser-claim-request-v1\0");
    for value in [
        run_id,
        ticket_hash,
        claim_nonce,
        descriptor.descriptor_base64url.as_str(),
        descriptor.signature_base64url.as_str(),
    ] {
        digest.update(value.as_bytes());
        digest.update(b"\0");
    }
    hex::encode(digest.finalize())
}

fn browser_release_phase_a_context(
    application: &JobApplication,
    ticket: &LocalRunTicket,
    run_id: &str,
    claim_nonce_sha256: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    release: &BrowserReleaseClaimBinding,
) -> Result<Option<AtsCertificationPhaseAContextRequest>> {
    if application.submission_mode != "auto_submit"
        || !application_has_frozen_ats_certification(application)
    {
        return Ok(None);
    }
    let browser_profile_id = ticket
        .payload
        .get("browserProfileId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 240)
        .ok_or_else(|| anyhow::anyhow!("certified local Browser claim has no browser profile"))?;
    let platform = match release.platform.as_str() {
        "darwin" => "macos",
        "windows" => "windows",
        _ => anyhow::bail!("certified local Browser claim has an invalid platform"),
    };
    let architecture = match release.architecture.as_str() {
        "arm64" => "arm64",
        "x64" => "x86_64",
        _ => anyhow::bail!("certified local Browser claim has an invalid architecture"),
    };
    Ok(Some(AtsCertificationPhaseAContextRequest {
        account_id: ticket.account_id.clone(),
        application_id: ticket.application_id.clone(),
        run_id: run_id.to_string(),
        browser_session_id: run_id.to_string(),
        browser_profile_id: browser_profile_id.to_string(),
        runtime_attestation: AtsCertificationRuntimeAttestation::Local {
            platform: platform.to_string(),
            architecture: architecture.to_string(),
            browser_release_manifest_sha256: release.manifest_sha256.clone(),
            browser_artifact_sha256: release.artifact_sha256.clone(),
            browser_build_descriptor_sha256: release.build_descriptor_sha256.clone(),
            automation_bundle_sha256: release.automation_bundle_sha256.clone(),
            playwright_version: descriptor.playwright_version.clone(),
            chromium_revision: descriptor.chromium_revision.clone(),
            chromium_executable_sha256: release.chromium_executable_sha256.clone(),
        },
        nonce_sha256: claim_nonce_sha256.to_string(),
    }))
}

fn sqlite_browser_runner_claim_operational_block(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<Option<BrowserLocalRunClaimDisposition>> {
    let hold_context = match operational_hold_context_for_application_sqlite_tx_after_authority(
        tx,
        account_id,
        application_id,
        employer_domain,
        Some("local"),
        None,
        None,
    ) {
        Ok(context) => context,
        Err(OperationalHoldError::Storage(error)) => return Err(error),
        Err(_) => return Ok(Some(BrowserLocalRunClaimDisposition::Rejected)),
    };
    match require_operational_capability_sqlite_tx(
        tx,
        OperationalCapability::RunnerClaim,
        &hold_context,
    ) {
        Ok(()) => Ok(None),
        Err(OperationalHoldError::Held(_)) => Ok(Some(
            BrowserLocalRunClaimDisposition::DistributionUnavailable,
        )),
        Err(OperationalHoldError::Storage(error)) => Err(error),
        Err(_) => Ok(Some(BrowserLocalRunClaimDisposition::Rejected)),
    }
}

fn postgres_browser_runner_claim_operational_block(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<Option<BrowserLocalRunClaimDisposition>> {
    let hold_context =
        match operational_hold_context_for_application_postgres_tx_after_authority_prelock(
            tx,
            account_id,
            application_id,
            employer_domain,
            Some("local"),
            None,
            None,
        ) {
            Ok(context) => context,
            Err(OperationalHoldError::Storage(error)) => return Err(error),
            Err(_) => return Ok(Some(BrowserLocalRunClaimDisposition::Rejected)),
        };
    match require_operational_capability_postgres_tx_after_authority_prelock(
        tx,
        OperationalCapability::RunnerClaim,
        &hold_context,
    ) {
        Ok(()) => Ok(None),
        Err(OperationalHoldError::Held(_)) => Ok(Some(
            BrowserLocalRunClaimDisposition::DistributionUnavailable,
        )),
        Err(OperationalHoldError::Storage(error)) => Err(error),
        Err(_) => Ok(Some(BrowserLocalRunClaimDisposition::Rejected)),
    }
}

fn sqlite_browser_replay_employer_domain(
    tx: &rusqlite::Transaction<'_>,
    ticket: &LocalRunTicket,
) -> Result<Option<OperationalHoldEmployerDomain>> {
    let application = tx
        .query_row(
            "SELECT id, job_id, application_json FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, ticket.application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(application_id, job_id, raw)| {
            parse_application_json(raw, &application_id, &job_id, "job application replay")
        })
        .transpose()?;
    let Some(application) = application else {
        return Ok(None);
    };
    if application.state == "submitted" {
        return submitted_execution_employer_domain(&ticket.account_id, &application).map(Some);
    }
    let current = resolve_current_execution_authority_sqlite_after_prelock(
        tx,
        &ticket.account_id,
        &application,
        ExecutionAuthorityRunner::Local,
    )?;
    Ok(if current.authorized {
        current.employer_domain
    } else {
        None
    })
}

fn postgres_browser_replay_employer_domain(
    tx: &mut postgres::Transaction<'_>,
    ticket: &LocalRunTicket,
) -> Result<Option<OperationalHoldEmployerDomain>> {
    let application = tx
        .query_opt(
            "SELECT id, job_id, application_json FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR SHARE",
            &[&ticket.account_id, &ticket.application_id],
        )?
        .map(|row| {
            let application_id: String = row.get(0);
            let job_id: String = row.get(1);
            parse_application_json(
                row.get(2),
                &application_id,
                &job_id,
                "job application replay",
            )
        })
        .transpose()?;
    let Some(application) = application else {
        return Ok(None);
    };
    if application.state == "submitted" {
        return submitted_execution_employer_domain(&ticket.account_id, &application).map(Some);
    }
    let current = resolve_current_execution_authority_postgres_after_prelock(
        tx,
        &ticket.account_id,
        &application,
        ExecutionAuthorityRunner::Local,
    )?;
    Ok(if current.authorized {
        current.employer_domain
    } else {
        None
    })
}

#[allow(clippy::too_many_arguments)]
fn sqlite_claim_local_run_with_browser_release<F>(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce_sha256: &str,
    claim_request_sha256: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    require_distribution_ready: bool,
    issue_response: &F,
) -> Result<BrowserLocalRunClaimDisposition>
where
    F: Fn(&LocalRunTicket, &BrowserReleaseClaimBinding) -> Result<Value>,
{
    if let Some(disposition) = sqlite_browser_claim_replay(
        tx,
        run_id,
        ticket_hash,
        claim_nonce_sha256,
        claim_request_sha256,
    )? {
        if let BrowserLocalRunClaimDisposition::Success(success) = &disposition {
            let Some(employer_domain) = sqlite_browser_replay_employer_domain(tx, &success.ticket)?
            else {
                return Ok(BrowserLocalRunClaimDisposition::Rejected);
            };
            if let Some(blocked) = sqlite_browser_runner_claim_operational_block(
                tx,
                &success.ticket.account_id,
                &success.ticket.application_id,
                &employer_domain,
            )? {
                return Ok(blocked);
            }
        }
        return Ok(disposition);
    }
    let account_id: Option<String> = tx
        .query_row(
            "SELECT account_id FROM jobs_local_run_tickets
              WHERE id = ?1 AND ticket_hash = ?2",
            params![run_id, ticket_hash],
            |row| row.get(0),
        )
        .optional()?;
    let Some(account_id) = account_id else {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    };
    if require_distribution_ready && !sqlite_runner_volume_fleet_distribution_ready(tx)? {
        return Ok(BrowserLocalRunClaimDisposition::DistributionUnavailable);
    }
    crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(tx, &account_id)?;
    let now = local_run_claim_db_now_sqlite(tx)?;
    let Some(authority) =
        sqlite_local_run_authority(tx, run_id, ticket_hash, now, LocalRunAuthorityPhase::Claim)?
    else {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    };
    let mut ticket = authority.ticket;
    if let Some(blocked) = sqlite_browser_runner_claim_operational_block(
        tx,
        &ticket.account_id,
        &ticket.application_id,
        &authority.employer_domain,
    )? {
        return Ok(blocked);
    }
    let reservation_status: Option<String> = tx
        .query_row(
            "SELECT status FROM jobs_attempt_reservations
              WHERE account_id = ?1 AND application_id = ?2",
            params![ticket.account_id, ticket.application_id],
            |row| row.get(0),
        )
        .optional()?;
    if reservation_status.as_deref() != Some("reserved") {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    }
    let Some(release) = sqlite_browser_release_for_claim_tx(
        tx,
        &ticket.account_id,
        descriptor,
        server_release_id,
        now,
    )?
    else {
        return Ok(BrowserLocalRunClaimDisposition::ReleaseUnavailable);
    };
    let response = issue_response(&ticket, &release)?;
    if !response.is_object() {
        anyhow::bail!("local Browser claim response must be an object")
    }
    let response_json = serde_json::to_string(&response)?;
    let response_sha256 = hex::encode(Sha256::digest(response_json.as_bytes()));
    let encrypted_response = encrypt_payload(&response_json)?;
    let binding_sha256 = browser_release_binding_sha256(&release);

    let (job_id, application_raw): (String, String) = tx.query_row(
        "SELECT job_id, application_json FROM jobs_applications
          WHERE account_id = ?1 AND id = ?2 AND state = 'queued'",
        params![ticket.account_id, ticket.application_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut application = parse_application_json(
        application_raw,
        &ticket.application_id,
        &job_id,
        "job application",
    )?;
    application.state = "running".to_string();
    application.updated_at_ms = now;
    let application_json = to_json(&application, "job application")?;
    let session_raw: String = tx.query_row(
        "SELECT session_json FROM jobs_browser_sessions
          WHERE account_id = ?1 AND id = ?2 AND runner = 'local' AND status = 'queued'",
        params![ticket.account_id, run_id],
        |row| row.get(0),
    )?;
    let mut session: BrowserSession = parse_json(session_raw, "browser session")?;
    session.status = "running".to_string();
    session.current_step = "Filling application".to_string();
    session.updated_at_ms = now;
    let session_json = to_json(&session, "browser session")?;

    tx.execute(
        "INSERT INTO jobs_local_run_release_bindings (
            run_id, account_id, application_id, binding_sha256,
            account_channel_assignment_sha256, account_channel_assignment_generation,
            channel, channel_head_revision, channel_transition_sha256,
            activation_sha256, activation_generation, trust_generation, trust_policy_sha256,
            channel_sequence, manifest_signature_set_sha256,
            activation_authorization_signature_set_sha256, manifest_sha256, artifact_id,
            release_id, build_id, app_version, protocol_version, platform, architecture,
            package_kind, build_descriptor_sha256, artifact_sha256, bound_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
            ?25, ?26, ?27, ?28
         )",
        params![
            run_id,
            ticket.account_id,
            ticket.application_id,
            binding_sha256,
            release.assignment_sha256,
            release.assignment_generation,
            release.channel,
            release.channel_head_revision,
            release.channel_transition_sha256,
            release.activation_sha256,
            release.activation_generation,
            release.trust_generation,
            release.trust_policy_sha256,
            release.channel_sequence,
            release.manifest_signature_set_sha256,
            release.activation_authorization_signature_set_sha256,
            release.manifest_sha256,
            release.artifact_id,
            release.release_id,
            release.build_id,
            release.app_version,
            release.protocol_version,
            release.platform,
            release.architecture,
            release.package_kind,
            release.build_descriptor_sha256,
            release.artifact_sha256,
            now,
        ],
    )?;
    if tx.execute(
        "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = ?3
          WHERE id = ?1 AND ticket_hash = ?2 AND status = 'queued' AND expires_at_ms > ?3",
        params![run_id, ticket_hash, now],
    )? != 1
        || tx.execute(
            "UPDATE jobs_applications SET state = 'running', application_json = ?3,
                    updated_at_ms = ?4
              WHERE account_id = ?1 AND id = ?2 AND state = 'queued'",
            params![
                ticket.account_id,
                ticket.application_id,
                application_json,
                now
            ],
        )? != 1
        || tx.execute(
            "UPDATE jobs_attempt_reservations SET status = 'running', updated_at_ms = ?3
              WHERE account_id = ?1 AND application_id = ?2 AND status = 'reserved'",
            params![ticket.account_id, ticket.application_id, now],
        )? != 1
        || tx.execute(
            "UPDATE jobs_browser_sessions SET status = 'running', session_json = ?3,
                    updated_at_ms = ?4
              WHERE account_id = ?1 AND id = ?2 AND runner = 'local' AND status = 'queued'",
            params![ticket.account_id, run_id, session_json, now],
        )? != 1
    {
        anyhow::bail!("local Browser claim authority changed during commit")
    }
    if let Some(request) = browser_release_phase_a_context(
        &application,
        &ticket,
        run_id,
        claim_nonce_sha256,
        descriptor,
        &release,
    )? {
        create_ats_application_certification_binding_from_context_sqlite_tx(tx, &request, now)?;
    }
    let event = json!({
        "application_id": ticket.application_id,
        "release": {
            "activation_sha256": release.activation_sha256,
            "manifest_sha256": release.manifest_sha256,
            "artifact_id": release.artifact_id,
            "descriptor_sha256": release.build_descriptor_sha256,
            "build_id": release.build_id,
            "protocol_version": release.protocol_version,
        }
    });
    tx.execute(
        "INSERT INTO jobs_run_events (
            id, account_id, run_id, event_type, event_json, created_at_ms
         ) VALUES (?1, ?2, ?3, 'local_browser_claimed', ?4, ?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            ticket.account_id,
            run_id,
            to_json(&event, "local Browser claim event")?,
            now,
        ],
    )?;
    tx.execute(
        "INSERT INTO jobs_local_run_claim_replays (
            run_id, account_id, claim_nonce_sha256, claim_request_sha256,
            claim_response_sha256, claim_response_secret, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            run_id,
            ticket.account_id,
            claim_nonce_sha256,
            claim_request_sha256,
            response_sha256,
            encrypted_response,
            now,
        ],
    )?;
    ticket.status = "claimed".to_string();
    ticket.updated_at_ms = now;
    Ok(BrowserLocalRunClaimDisposition::Success(Box::new(
        BrowserLocalRunClaimSuccess {
            ticket,
            response,
            response_json,
            release,
            replayed: false,
        },
    )))
}

fn sqlite_browser_claim_replay(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce_sha256: &str,
    claim_request_sha256: &str,
) -> Result<Option<BrowserLocalRunClaimDisposition>> {
    let replay: Option<(LocalRunTicket, String, String, String, String)> = tx
        .query_row(
            "SELECT ticket.id, ticket.account_id, ticket.application_id,
                    ticket.ticket_hash, ticket.ticket_secret, ticket.payload_json,
                    ticket.status, ticket.expires_at_ms, ticket.created_at_ms,
                    ticket.updated_at_ms, replay.claim_nonce_sha256,
                    replay.claim_request_sha256, replay.claim_response_sha256,
                    replay.claim_response_secret
               FROM jobs_local_run_claim_replays replay
               JOIN jobs_local_run_tickets ticket
                 ON ticket.id = replay.run_id
                AND ticket.account_id = replay.account_id
              WHERE replay.run_id = ?1 AND ticket.ticket_hash = ?2",
            params![run_id, ticket_hash],
            |row| {
                Ok((
                    local_run_ticket_from_sqlite_row(row)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                ))
            },
        )
        .optional()?;
    let Some((ticket, stored_nonce, stored_request, stored_response_sha, secret)) = replay else {
        return Ok(None);
    };
    if !browser_release_constant_time_eq(&stored_nonce, claim_nonce_sha256)
        || !browser_release_constant_time_eq(&stored_request, claim_request_sha256)
    {
        return Ok(Some(BrowserLocalRunClaimDisposition::ConflictingReplay));
    }
    let response_json = decrypt_payload(&secret).context("decrypt local Browser claim replay")?;
    let actual_response_sha = hex::encode(Sha256::digest(response_json.as_bytes()));
    if !browser_release_constant_time_eq(&stored_response_sha, &actual_response_sha) {
        anyhow::bail!("local Browser claim replay response digest mismatch")
    }
    let response: Value = serde_json::from_str(&response_json)
        .context("parse local Browser claim replay response")?;
    let release = sqlite_browser_release_binding(tx, run_id, &ticket.account_id)?
        .context("local Browser claim replay is missing release binding")?;
    Ok(Some(BrowserLocalRunClaimDisposition::Success(Box::new(
        BrowserLocalRunClaimSuccess {
            ticket,
            response,
            response_json,
            release,
            replayed: true,
        },
    ))))
}

fn sqlite_browser_release_binding(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    account_id: &str,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    tx.query_row(
        "SELECT b.account_channel_assignment_sha256,
                b.account_channel_assignment_generation, b.channel,
                b.channel_head_revision, b.channel_transition_sha256,
                b.activation_sha256, b.activation_generation, b.trust_generation,
                b.trust_policy_sha256, b.channel_sequence,
                b.manifest_signature_set_sha256,
                b.activation_authorization_signature_set_sha256, b.manifest_sha256,
                m.release_sequence, b.artifact_id, b.artifact_sha256,
                a.artifact_url, a.artifact_filename, a.artifact_size_bytes,
                b.package_kind, b.release_id, b.build_id, b.app_version,
                b.protocol_version, b.platform, b.architecture,
                b.build_descriptor_sha256, runtime.automation_bundle_sha256,
                runtime.chromium_executable_sha256, m.published_at_ms
           FROM jobs_local_run_release_bindings b
           JOIN jobs_browser_release_manifests m
             ON m.manifest_sha256 = b.manifest_sha256
           JOIN jobs_browser_release_artifacts a
             ON a.manifest_sha256 = b.manifest_sha256 AND a.artifact_id = b.artifact_id
           JOIN jobs_browser_release_artifact_runtime_components runtime
             ON runtime.manifest_sha256 = a.manifest_sha256
            AND runtime.artifact_id = a.artifact_id
            AND runtime.build_descriptor_sha256 = a.build_descriptor_sha256
            AND runtime.artifact_sha256 = a.artifact_sha256
            AND runtime.platform = a.platform
            AND runtime.architecture = a.architecture
            AND runtime.package_kind = a.package_kind
          WHERE b.run_id = ?1 AND b.account_id = ?2",
        params![run_id, account_id],
        browser_release_binding_from_sqlite_row,
    )
    .optional()
    .context("get local Browser release binding")
}

fn browser_release_binding_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BrowserReleaseClaimBinding> {
    Ok(BrowserReleaseClaimBinding {
        assignment_sha256: row.get(0)?,
        assignment_generation: row.get(1)?,
        channel: row.get(2)?,
        channel_head_revision: row.get(3)?,
        channel_transition_sha256: row.get(4)?,
        activation_sha256: row.get(5)?,
        activation_generation: row.get(6)?,
        trust_generation: row.get(7)?,
        trust_policy_sha256: row.get(8)?,
        channel_sequence: row.get(9)?,
        manifest_signature_set_sha256: row.get(10)?,
        activation_authorization_signature_set_sha256: row.get(11)?,
        manifest_sha256: row.get(12)?,
        release_sequence: row.get(13)?,
        artifact_id: row.get(14)?,
        artifact_sha256: row.get(15)?,
        artifact_url: row.get(16)?,
        artifact_filename: row.get(17)?,
        artifact_size_bytes: row.get(18)?,
        package_kind: row.get(19)?,
        release_id: row.get(20)?,
        build_id: row.get(21)?,
        app_version: row.get(22)?,
        protocol_version: row.get(23)?,
        platform: row.get(24)?,
        architecture: row.get(25)?,
        build_descriptor_sha256: row.get(26)?,
        automation_bundle_sha256: row.get(27)?,
        chromium_executable_sha256: row.get(28)?,
        published_at_ms: row.get(29)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn postgres_claim_local_run_with_browser_release<F>(
    tx: &mut postgres::Transaction<'_>,
    prelocked_account_id: &str,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce_sha256: &str,
    claim_request_sha256: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    require_distribution_ready: bool,
    issue_response: &F,
) -> Result<BrowserLocalRunClaimDisposition>
where
    F: Fn(&LocalRunTicket, &BrowserReleaseClaimBinding) -> Result<Value>,
{
    if let Some(disposition) = postgres_browser_claim_replay(
        tx,
        run_id,
        ticket_hash,
        claim_nonce_sha256,
        claim_request_sha256,
    )? {
        if let BrowserLocalRunClaimDisposition::Success(success) = &disposition {
            if success.ticket.account_id != prelocked_account_id {
                return Ok(BrowserLocalRunClaimDisposition::Rejected);
            }
            let Some(employer_domain) =
                postgres_browser_replay_employer_domain(tx, &success.ticket)?
            else {
                return Ok(BrowserLocalRunClaimDisposition::Rejected);
            };
            if let Some(blocked) = postgres_browser_runner_claim_operational_block(
                tx,
                &success.ticket.account_id,
                &success.ticket.application_id,
                &employer_domain,
            )? {
                return Ok(blocked);
            }
        }
        return Ok(disposition);
    }
    if require_distribution_ready && !postgres_runner_volume_fleet_distribution_ready(tx)? {
        return Ok(BrowserLocalRunClaimDisposition::DistributionUnavailable);
    }
    tx.query_one(
        "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
        &[&prelocked_account_id],
    )?;
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
        tx,
        prelocked_account_id,
    )?;
    let Some(authority_prelock) = postgres_local_run_authority_prelock(
        tx,
        prelocked_account_id,
        run_id,
        ticket_hash,
        LocalRunAuthorityPhase::Claim,
    )?
    else {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    };
    let ticket_application_id = authority_prelock.ticket.application_id.clone();
    let mut application = authority_prelock.application.clone();
    let mut session = authority_prelock.session.clone();
    postgres_lock_browser_release_registry_shared(tx)?;
    let now = local_run_claim_db_now_postgres(tx)?;
    let Some(authority) =
        postgres_local_run_authority_after_prelock_at_ms(tx, authority_prelock, now)?
    else {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    };
    if authority.ticket.account_id != prelocked_account_id
        || authority.ticket.application_id != ticket_application_id
    {
        return Ok(BrowserLocalRunClaimDisposition::Rejected);
    }
    let mut ticket = authority.ticket;
    if let Some(blocked) = postgres_browser_runner_claim_operational_block(
        tx,
        &ticket.account_id,
        &ticket.application_id,
        &authority.employer_domain,
    )? {
        return Ok(blocked);
    }
    let Some(release) = postgres_browser_release_for_claim_tx(
        tx,
        &ticket.account_id,
        descriptor,
        server_release_id,
        now,
    )?
    else {
        return Ok(BrowserLocalRunClaimDisposition::ReleaseUnavailable);
    };
    let response = issue_response(&ticket, &release)?;
    if !response.is_object() {
        anyhow::bail!("local Browser claim response must be an object")
    }
    let response_json = serde_json::to_string(&response)?;
    let response_sha256 = hex::encode(Sha256::digest(response_json.as_bytes()));
    let encrypted_response = encrypt_payload(&response_json)?;
    let binding_sha256 = browser_release_binding_sha256(&release);

    application.state = "running".to_string();
    application.updated_at_ms = now;
    let application_json = to_json(&application, "job application")?;
    session.status = "running".to_string();
    session.current_step = "Filling application".to_string();
    session.updated_at_ms = now;
    let session_json = to_json(&session, "browser session")?;

    tx.execute(
        "INSERT INTO jobs_local_run_release_bindings (
            run_id, account_id, application_id, binding_sha256,
            account_channel_assignment_sha256, account_channel_assignment_generation,
            channel, channel_head_revision, channel_transition_sha256,
            activation_sha256, activation_generation, trust_generation, trust_policy_sha256,
            channel_sequence, manifest_signature_set_sha256,
            activation_authorization_signature_set_sha256, manifest_sha256, artifact_id,
            release_id, build_id, app_version, protocol_version, platform, architecture,
            package_kind, build_descriptor_sha256, artifact_sha256, bound_at_ms
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24,
            $25, $26, $27, $28
         )",
        &[
            &run_id,
            &ticket.account_id,
            &ticket.application_id,
            &binding_sha256,
            &release.assignment_sha256,
            &release.assignment_generation,
            &release.channel,
            &release.channel_head_revision,
            &release.channel_transition_sha256,
            &release.activation_sha256,
            &release.activation_generation,
            &release.trust_generation,
            &release.trust_policy_sha256,
            &release.channel_sequence,
            &release.manifest_signature_set_sha256,
            &release.activation_authorization_signature_set_sha256,
            &release.manifest_sha256,
            &release.artifact_id,
            &release.release_id,
            &release.build_id,
            &release.app_version,
            &release.protocol_version,
            &release.platform,
            &release.architecture,
            &release.package_kind,
            &release.build_descriptor_sha256,
            &release.artifact_sha256,
            &now,
        ],
    )?;
    if tx.execute(
        "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = $3
          WHERE id = $1 AND ticket_hash = $2 AND status = 'queued' AND expires_at_ms > $3",
        &[&run_id, &ticket_hash, &now],
    )? != 1
        || tx.execute(
            "UPDATE jobs_applications SET state = 'running', application_json = $3,
                    updated_at_ms = $4
              WHERE account_id = $1 AND id = $2 AND state = 'queued'",
            &[
                &ticket.account_id,
                &ticket.application_id,
                &application_json,
                &now,
            ],
        )? != 1
        || tx.execute(
            "UPDATE jobs_attempt_reservations SET status = 'running', updated_at_ms = $3
              WHERE account_id = $1 AND application_id = $2 AND status = 'reserved'",
            &[&ticket.account_id, &ticket.application_id, &now],
        )? != 1
        || tx.execute(
            "UPDATE jobs_browser_sessions SET status = 'running', session_json = $3,
                    updated_at_ms = $4
              WHERE account_id = $1 AND id = $2 AND runner = 'local' AND status = 'queued'",
            &[&ticket.account_id, &run_id, &session_json, &now],
        )? != 1
    {
        anyhow::bail!("local Browser claim authority changed during commit")
    }
    if let Some(request) = browser_release_phase_a_context(
        &application,
        &ticket,
        run_id,
        claim_nonce_sha256,
        descriptor,
        &release,
    )? {
        create_ats_application_certification_binding_from_context_postgres_tx_after_prelock(
            tx, &request, now,
        )?;
    }
    let event = json!({
        "application_id": ticket.application_id,
        "release": {
            "activation_sha256": release.activation_sha256,
            "manifest_sha256": release.manifest_sha256,
            "artifact_id": release.artifact_id,
            "descriptor_sha256": release.build_descriptor_sha256,
            "build_id": release.build_id,
            "protocol_version": release.protocol_version,
        }
    });
    let event_json = to_json(&event, "local Browser claim event")?;
    tx.execute(
        "INSERT INTO jobs_run_events (
            id, account_id, run_id, event_type, event_json, created_at_ms
         ) VALUES ($1, $2, $3, 'local_browser_claimed', $4, $5)",
        &[
            &uuid::Uuid::new_v4().to_string(),
            &ticket.account_id,
            &run_id,
            &event_json,
            &now,
        ],
    )?;
    tx.execute(
        "INSERT INTO jobs_local_run_claim_replays (
            run_id, account_id, claim_nonce_sha256, claim_request_sha256,
            claim_response_sha256, claim_response_secret, created_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &run_id,
            &ticket.account_id,
            &claim_nonce_sha256,
            &claim_request_sha256,
            &response_sha256,
            &encrypted_response,
            &now,
        ],
    )?;
    ticket.status = "claimed".to_string();
    ticket.updated_at_ms = now;
    Ok(BrowserLocalRunClaimDisposition::Success(Box::new(
        BrowserLocalRunClaimSuccess {
            ticket,
            response,
            response_json,
            release,
            replayed: false,
        },
    )))
}

fn postgres_lock_browser_release_registry_shared(
    transaction: &mut postgres::Transaction<'_>,
) -> Result<()> {
    transaction.query_one(
        "SELECT pg_advisory_xact_lock_shared(\
         hashtextextended('jobs-browser-release-registry', 0))",
        &[],
    )?;
    Ok(())
}

fn postgres_browser_claim_replay(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    claim_nonce_sha256: &str,
    claim_request_sha256: &str,
) -> Result<Option<BrowserLocalRunClaimDisposition>> {
    let replay = tx.query_opt(
        "SELECT ticket.id, ticket.account_id, ticket.application_id,
                ticket.ticket_hash, ticket.ticket_secret, ticket.payload_json,
                ticket.status, ticket.expires_at_ms, ticket.created_at_ms,
                ticket.updated_at_ms, replay.claim_nonce_sha256,
                replay.claim_request_sha256, replay.claim_response_sha256,
                replay.claim_response_secret
           FROM jobs_local_run_claim_replays replay
           JOIN jobs_local_run_tickets ticket
             ON ticket.id = replay.run_id
            AND ticket.account_id = replay.account_id
          WHERE replay.run_id = $1 AND ticket.ticket_hash = $2
          FOR UPDATE OF replay",
        &[&run_id, &ticket_hash],
    )?;
    let Some(replay) = replay else {
        return Ok(None);
    };
    let stored_nonce: String = replay.get(10);
    let stored_request: String = replay.get(11);
    let stored_response_sha: String = replay.get(12);
    let secret: String = replay.get(13);
    let ticket = local_run_ticket_from_pg_row(replay)?;
    if !browser_release_constant_time_eq(&stored_nonce, claim_nonce_sha256)
        || !browser_release_constant_time_eq(&stored_request, claim_request_sha256)
    {
        return Ok(Some(BrowserLocalRunClaimDisposition::ConflictingReplay));
    }
    let response_json = decrypt_payload(&secret).context("decrypt local Browser claim replay")?;
    let actual_response_sha = hex::encode(Sha256::digest(response_json.as_bytes()));
    if !browser_release_constant_time_eq(&stored_response_sha, &actual_response_sha) {
        anyhow::bail!("local Browser claim replay response digest mismatch")
    }
    let response: Value = serde_json::from_str(&response_json)
        .context("parse local Browser claim replay response")?;
    let release = postgres_browser_release_binding(tx, run_id, &ticket.account_id)?
        .context("local Browser claim replay is missing release binding")?;
    Ok(Some(BrowserLocalRunClaimDisposition::Success(Box::new(
        BrowserLocalRunClaimSuccess {
            ticket,
            response,
            response_json,
            release,
            replayed: true,
        },
    ))))
}

fn postgres_browser_release_binding(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    account_id: &str,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    Ok(tx
        .query_opt(
            "SELECT b.account_channel_assignment_sha256,
                    b.account_channel_assignment_generation, b.channel,
                    b.channel_head_revision, b.channel_transition_sha256,
                    b.activation_sha256, b.activation_generation, b.trust_generation,
                    b.trust_policy_sha256, b.channel_sequence,
                    b.manifest_signature_set_sha256,
                    b.activation_authorization_signature_set_sha256, b.manifest_sha256,
                    m.release_sequence, b.artifact_id, b.artifact_sha256,
                    a.artifact_url, a.artifact_filename, a.artifact_size_bytes,
                    b.package_kind, b.release_id, b.build_id, b.app_version,
                    b.protocol_version, b.platform, b.architecture,
                    b.build_descriptor_sha256, runtime.automation_bundle_sha256,
                    runtime.chromium_executable_sha256, m.published_at_ms
               FROM jobs_local_run_release_bindings b
               JOIN jobs_browser_release_manifests m
                 ON m.manifest_sha256 = b.manifest_sha256
               JOIN jobs_browser_release_artifacts a
                 ON a.manifest_sha256 = b.manifest_sha256
                AND a.artifact_id = b.artifact_id
               JOIN jobs_browser_release_artifact_runtime_components runtime
                 ON runtime.manifest_sha256 = a.manifest_sha256
                AND runtime.artifact_id = a.artifact_id
                AND runtime.build_descriptor_sha256 = a.build_descriptor_sha256
                AND runtime.artifact_sha256 = a.artifact_sha256
                AND runtime.platform = a.platform
                AND runtime.architecture = a.architecture
                AND runtime.package_kind = a.package_kind
              WHERE b.run_id = $1 AND b.account_id = $2",
            &[&run_id, &account_id],
        )?
        .map(browser_release_binding_from_pg_row))
}

fn browser_release_binding_from_pg_row(row: postgres::Row) -> BrowserReleaseClaimBinding {
    BrowserReleaseClaimBinding {
        assignment_sha256: row.get(0),
        assignment_generation: row.get(1),
        channel: row.get(2),
        channel_head_revision: row.get(3),
        channel_transition_sha256: row.get(4),
        activation_sha256: row.get(5),
        activation_generation: row.get(6),
        trust_generation: row.get(7),
        trust_policy_sha256: row.get(8),
        channel_sequence: row.get(9),
        manifest_signature_set_sha256: row.get(10),
        activation_authorization_signature_set_sha256: row.get(11),
        manifest_sha256: row.get(12),
        release_sequence: row.get(13),
        artifact_id: row.get(14),
        artifact_sha256: row.get(15),
        artifact_url: row.get(16),
        artifact_filename: row.get(17),
        artifact_size_bytes: row.get(18),
        package_kind: row.get(19),
        release_id: row.get(20),
        build_id: row.get(21),
        app_version: row.get(22),
        protocol_version: row.get(23),
        platform: row.get(24),
        architecture: row.get(25),
        build_descriptor_sha256: row.get(26),
        automation_bundle_sha256: row.get(27),
        chromium_executable_sha256: row.get(28),
        published_at_ms: row.get(29),
    }
}

fn browser_release_constant_time_eq(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).unwrap_u8() == 1
}

fn sqlite_runner_volume_fleet_distribution_ready(tx: &rusqlite::Transaction<'_>) -> Result<bool> {
    let ready: i64 = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM jobs_runner_volume_fleet_state f
             WHERE f.singleton_id = 1
               AND f.cutover_state = 'ready'
               AND f.legacy_inventory_state = 'ready'
               AND f.unresolved_legacy_volume_count = 0
               AND f.legacy_inventory_reconciliation_id IS NOT NULL
               AND f.legacy_inventory_authority_id IS NOT NULL
               AND f.legacy_inventory_authority_sha256 IS NOT NULL
               AND f.legacy_inventory_root_count = 0
               AND f.legacy_inventory_root_set_sha256 = ?1
               AND f.cutover_enrollment_generation = f.enrollment_generation
               AND f.cutover_purge_generation = f.purge_generation
               AND f.cutover_tombstone_generation = f.tombstone_generation
               AND f.cutover_destruction_generation = f.destruction_generation
               AND f.cutover_legacy_reconciliation_generation =
                   f.legacy_reconciliation_generation
               AND f.cutover_storage_attestation_generation =
                   f.storage_attestation_generation
               AND f.cutover_storage_attestation_count = f.storage_attestation_count
               AND f.cutover_storage_attestation_set_sha256 =
                   f.storage_attestation_set_sha256
               AND f.cutover_legacy_inventory_generation = f.legacy_inventory_generation
               AND f.cutover_legacy_inventory_reconciliation_id =
                   f.legacy_inventory_reconciliation_id
               AND f.cutover_legacy_inventory_authority_id =
                   f.legacy_inventory_authority_id
               AND f.cutover_legacy_inventory_authority_sha256 =
                   f.legacy_inventory_authority_sha256
               AND f.cutover_legacy_inventory_root_count = f.legacy_inventory_root_count
               AND f.cutover_legacy_inventory_root_set_sha256 =
                   f.legacy_inventory_root_set_sha256
               AND f.cutover_unresolved_legacy_volume_count = 0
               AND f.cutover_evidence_ref IS NOT NULL
               AND f.cutover_evidence_sha256 IS NOT NULL
               AND f.cutover_authorized_by IS NOT NULL
               AND f.cutover_at_ms IS NOT NULL
               AND f.storage_attestation_count = (
                 SELECT COUNT(*) FROM jobs_runner_volumes v
                  WHERE v.status <> 'destroyed'
               )
               AND f.cutover_storage_attestation_count = f.storage_attestation_count
               AND f.cutover_non_destroyed_volume_count = (
                 SELECT COUNT(*) FROM jobs_runner_volumes v
                  WHERE v.status <> 'destroyed'
               )
               AND f.cutover_destruction_count = (
                 SELECT COUNT(*) FROM jobs_runner_volume_destructions
               )
               AND (
                 SELECT COUNT(*) FROM jobs_runner_volumes v
                  WHERE v.status IN ('active', 'suspended', 'retired')
                    AND v.required_tombstone_generation =
                        v.reconciled_tombstone_generation
                    AND EXISTS (
                      SELECT 1 FROM jobs_runner_volume_storage_attestations a
                       WHERE a.volume_id = v.volume_id
                         AND a.enrollment_epoch = v.current_epoch
                         AND a.process_instance_id = v.active_instance_id
                         AND a.enrollment_generation = v.enrollment_generation
                         AND a.required_tombstone_generation =
                             v.required_tombstone_generation
                         AND a.reconciled_tombstone_generation =
                             v.reconciled_tombstone_generation
                         AND NOT EXISTS (
                           SELECT 1 FROM jobs_runner_volume_storage_attestations newer
                            WHERE newer.volume_id = a.volume_id
                              AND newer.enrollment_epoch = a.enrollment_epoch
                              AND newer.attestation_generation > a.attestation_generation
                         )
                    )
               ) = (
                 SELECT COUNT(*) FROM jobs_runner_volumes v
                  WHERE v.status <> 'destroyed'
               )
          )",
        params![EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256],
        |row| row.get(0),
    )?;
    Ok(ready != 0)
}

fn postgres_runner_volume_fleet_distribution_ready(
    tx: &mut postgres::Transaction<'_>,
) -> Result<bool> {
    if tx
        .query_opt(
            "SELECT singleton_id FROM jobs_runner_volume_fleet_state
              WHERE singleton_id = 1 FOR SHARE",
            &[],
        )?
        .is_none()
    {
        return Ok(false);
    }
    Ok(tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_runner_volume_fleet_state f
                 WHERE f.singleton_id = 1
                   AND f.cutover_state = 'ready'
                   AND f.legacy_inventory_state = 'ready'
                   AND f.unresolved_legacy_volume_count = 0
                   AND f.legacy_inventory_reconciliation_id IS NOT NULL
                   AND f.legacy_inventory_authority_id IS NOT NULL
                   AND f.legacy_inventory_authority_sha256 IS NOT NULL
                   AND f.legacy_inventory_root_count = 0
                   AND f.legacy_inventory_root_set_sha256 = $1
                   AND f.cutover_enrollment_generation = f.enrollment_generation
                   AND f.cutover_purge_generation = f.purge_generation
                   AND f.cutover_tombstone_generation = f.tombstone_generation
                   AND f.cutover_destruction_generation = f.destruction_generation
                   AND f.cutover_legacy_reconciliation_generation =
                       f.legacy_reconciliation_generation
                   AND f.cutover_storage_attestation_generation =
                       f.storage_attestation_generation
                   AND f.cutover_storage_attestation_count = f.storage_attestation_count
                   AND f.cutover_storage_attestation_set_sha256 =
                       f.storage_attestation_set_sha256
                   AND f.cutover_legacy_inventory_generation = f.legacy_inventory_generation
                   AND f.cutover_legacy_inventory_reconciliation_id =
                       f.legacy_inventory_reconciliation_id
                   AND f.cutover_legacy_inventory_authority_id =
                       f.legacy_inventory_authority_id
                   AND f.cutover_legacy_inventory_authority_sha256 =
                       f.legacy_inventory_authority_sha256
                   AND f.cutover_legacy_inventory_root_count =
                       f.legacy_inventory_root_count
                   AND f.cutover_legacy_inventory_root_set_sha256 =
                       f.legacy_inventory_root_set_sha256
                   AND f.cutover_unresolved_legacy_volume_count = 0
                   AND f.cutover_evidence_ref IS NOT NULL
                   AND f.cutover_evidence_sha256 IS NOT NULL
                   AND f.cutover_authorized_by IS NOT NULL
                   AND f.cutover_at_ms IS NOT NULL
                   AND f.storage_attestation_count = (
                     SELECT COUNT(*) FROM jobs_runner_volumes v
                      WHERE v.status <> 'destroyed'
                   )
                   AND f.cutover_storage_attestation_count = f.storage_attestation_count
                   AND f.cutover_non_destroyed_volume_count = (
                     SELECT COUNT(*) FROM jobs_runner_volumes v
                      WHERE v.status <> 'destroyed'
                   )
                   AND f.cutover_destruction_count = (
                     SELECT COUNT(*) FROM jobs_runner_volume_destructions
                   )
                   AND (
                     SELECT COUNT(*) FROM jobs_runner_volumes v
                      WHERE v.status IN ('active', 'suspended', 'retired')
                        AND v.required_tombstone_generation =
                            v.reconciled_tombstone_generation
                        AND EXISTS (
                          SELECT 1 FROM jobs_runner_volume_storage_attestations a
                           WHERE a.volume_id = v.volume_id
                             AND a.enrollment_epoch = v.current_epoch
                             AND a.process_instance_id = v.active_instance_id
                             AND a.enrollment_generation = v.enrollment_generation
                             AND a.required_tombstone_generation =
                                 v.required_tombstone_generation
                             AND a.reconciled_tombstone_generation =
                                 v.reconciled_tombstone_generation
                             AND NOT EXISTS (
                               SELECT 1 FROM jobs_runner_volume_storage_attestations newer
                                WHERE newer.volume_id = a.volume_id
                                  AND newer.enrollment_epoch = a.enrollment_epoch
                                  AND newer.attestation_generation >
                                      a.attestation_generation
                             )
                        )
                   ) = (
                     SELECT COUNT(*) FROM jobs_runner_volumes v
                      WHERE v.status <> 'destroyed'
                   )
              )",
            &[&EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256],
        )?
        .get(0))
}

fn sqlite_bound_browser_release_submit_allowed_at_ms(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    server_release_id: &str,
    now: i64,
) -> Result<bool> {
    let allowed: i64 = tx.query_row(
        "SELECT EXISTS(
            SELECT 1
              FROM jobs_local_run_release_bindings b
              JOIN jobs_browser_release_artifacts a
                ON a.manifest_sha256 = b.manifest_sha256
               AND a.artifact_id = b.artifact_id
              JOIN jobs_browser_release_artifact_runtime_components runtime
                ON runtime.manifest_sha256 = a.manifest_sha256
               AND runtime.artifact_id = a.artifact_id
               AND runtime.build_descriptor_sha256 = a.build_descriptor_sha256
               AND runtime.artifact_sha256 = a.artifact_sha256
               AND runtime.platform = a.platform
               AND runtime.architecture = a.architecture
               AND runtime.package_kind = a.package_kind
              JOIN jobs_browser_release_manifests m
                ON m.manifest_sha256 = b.manifest_sha256
              JOIN jobs_browser_release_activations act
                ON act.activation_sha256 = b.activation_sha256
               AND act.manifest_sha256 = b.manifest_sha256
               AND act.manifest_signature_set_sha256 = b.manifest_signature_set_sha256
               AND act.authorization_signature_set_sha256 =
                   b.activation_authorization_signature_set_sha256
              JOIN jobs_browser_release_trust_policies p
                ON p.policy_sha256 = b.trust_policy_sha256
               AND p.trust_generation = b.trust_generation
              JOIN jobs_browser_release_channel_heads h
                ON h.channel = b.channel
               AND h.head_revision = b.channel_head_revision
               AND h.current_transition_sha256 = b.channel_transition_sha256
               AND h.current_activation_sha256 = b.activation_sha256
               AND h.current_manifest_sha256 = b.manifest_sha256
               AND h.current_trust_generation = b.trust_generation
               AND h.current_channel_sequence = b.channel_sequence
              JOIN jobs_browser_account_channel_assignments account_assignment
                ON account_assignment.assignment_sha256 =
                   b.account_channel_assignment_sha256
               AND account_assignment.account_id = b.account_id
               AND account_assignment.assignment_generation =
                   b.account_channel_assignment_generation
               AND account_assignment.channel = b.channel
             WHERE b.run_id = ?1
               AND account_assignment.assignment_generation = (
                 SELECT MAX(latest_assignment.assignment_generation)
                   FROM jobs_browser_account_channel_assignments latest_assignment
                  WHERE latest_assignment.account_id = b.account_id
               )
               AND p.trust_generation = (
                 SELECT MAX(trust_generation)
                   FROM jobs_browser_release_trust_policies
               )
               AND p.valid_from_ms <= ?2 AND p.expires_at_ms > ?2
               AND act.expires_at_ms > ?2
               AND EXISTS (
                 SELECT 1 FROM json_each(act.accepted_server_release_ids_json) accepted
                  WHERE accepted.type = 'text' AND accepted.value = ?3
               )
               AND NOT EXISTS (
                 SELECT 1 FROM jobs_browser_release_revocations r
                  WHERE (r.subject_kind = 'build-descriptor'
                         AND r.subject_id = b.build_descriptor_sha256
                         AND r.subject_sha256 = b.build_descriptor_sha256)
                     OR (r.subject_kind = 'manifest'
                         AND r.subject_id = m.manifest_id
                         AND r.subject_sha256 = b.manifest_sha256)
                     OR (r.subject_kind = 'release'
                         AND r.subject_id = b.release_id)
                     OR (r.subject_kind = 'artifact' AND EXISTS (
                           SELECT 1 FROM jobs_browser_release_artifacts sibling
                            WHERE sibling.manifest_sha256 = b.manifest_sha256
                              AND sibling.build_descriptor_sha256 =
                                  b.build_descriptor_sha256
                              AND sibling.artifact_id = r.subject_id
                              AND sibling.artifact_sha256 = r.subject_sha256
                         ))
                     OR (r.subject_kind = 'signing-key' AND (
                           r.subject_id = a.build_descriptor_signing_key_id
                           OR EXISTS (
                             SELECT 1 FROM jobs_browser_release_signatures s
                              WHERE s.key_id = r.subject_id
                                AND s.signature_set_sha256 IN (
                                  b.manifest_signature_set_sha256,
                                  b.activation_authorization_signature_set_sha256,
                                  p.authorization_signature_set_sha256
                                )
                           )
                         ))
               )
               AND NOT EXISTS (
                 SELECT 1 FROM jobs_browser_release_trust_keys k
                  WHERE k.policy_sha256 = p.policy_sha256
                    AND k.state = 'revoked'
                    AND (
                      k.key_id = a.build_descriptor_signing_key_id
                      OR EXISTS (
                        SELECT 1 FROM jobs_browser_release_signatures s
                         WHERE s.key_id = k.key_id
                           AND s.signature_set_sha256 IN (
                             b.manifest_signature_set_sha256,
                             b.activation_authorization_signature_set_sha256,
                             p.authorization_signature_set_sha256
                           )
                      )
                    )
               )
          )",
        params![run_id, now, server_release_id],
        |row| row.get(0),
    )?;
    Ok(allowed != 0)
}

fn postgres_bound_browser_release_submit_allowed_at_ms(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    server_release_id: &str,
    now: i64,
) -> Result<bool> {
    Ok(tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1
                  FROM jobs_local_run_release_bindings b
                  JOIN jobs_browser_release_artifacts a
                    ON a.manifest_sha256 = b.manifest_sha256
                   AND a.artifact_id = b.artifact_id
                  JOIN jobs_browser_release_artifact_runtime_components runtime
                    ON runtime.manifest_sha256 = a.manifest_sha256
                   AND runtime.artifact_id = a.artifact_id
                   AND runtime.build_descriptor_sha256 = a.build_descriptor_sha256
                   AND runtime.artifact_sha256 = a.artifact_sha256
                   AND runtime.platform = a.platform
                   AND runtime.architecture = a.architecture
                   AND runtime.package_kind = a.package_kind
                  JOIN jobs_browser_release_manifests m
                    ON m.manifest_sha256 = b.manifest_sha256
                  JOIN jobs_browser_release_activations act
                    ON act.activation_sha256 = b.activation_sha256
                   AND act.manifest_sha256 = b.manifest_sha256
                   AND act.manifest_signature_set_sha256 = b.manifest_signature_set_sha256
                   AND act.authorization_signature_set_sha256 =
                       b.activation_authorization_signature_set_sha256
                  JOIN jobs_browser_release_trust_policies p
                    ON p.policy_sha256 = b.trust_policy_sha256
                   AND p.trust_generation = b.trust_generation
                  JOIN jobs_browser_release_channel_heads h
                    ON h.channel = b.channel
                   AND h.head_revision = b.channel_head_revision
                   AND h.current_transition_sha256 = b.channel_transition_sha256
                   AND h.current_activation_sha256 = b.activation_sha256
                   AND h.current_manifest_sha256 = b.manifest_sha256
                   AND h.current_trust_generation = b.trust_generation
                   AND h.current_channel_sequence = b.channel_sequence
                  JOIN jobs_browser_account_channel_assignments account_assignment
                    ON account_assignment.assignment_sha256 =
                       b.account_channel_assignment_sha256
                   AND account_assignment.account_id = b.account_id
                   AND account_assignment.assignment_generation =
                       b.account_channel_assignment_generation
                   AND account_assignment.channel = b.channel
                 WHERE b.run_id = $1
                   AND account_assignment.assignment_generation = (
                     SELECT MAX(latest_assignment.assignment_generation)
                       FROM jobs_browser_account_channel_assignments latest_assignment
                      WHERE latest_assignment.account_id = b.account_id
                   )
                   AND p.trust_generation = (
                     SELECT MAX(trust_generation)
                       FROM jobs_browser_release_trust_policies
                   )
                   AND p.valid_from_ms <= $2 AND p.expires_at_ms > $2
                   AND act.expires_at_ms > $2
                   AND EXISTS (
                     SELECT 1
                       FROM jsonb_array_elements_text(
                         act.accepted_server_release_ids_json::jsonb
                       ) AS accepted(value)
                      WHERE accepted.value = $3
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM jobs_browser_release_revocations r
                      WHERE (r.subject_kind = 'build-descriptor'
                             AND r.subject_id = b.build_descriptor_sha256
                             AND r.subject_sha256 = b.build_descriptor_sha256)
                         OR (r.subject_kind = 'manifest'
                             AND r.subject_id = m.manifest_id
                             AND r.subject_sha256 = b.manifest_sha256)
                         OR (r.subject_kind = 'release'
                             AND r.subject_id = b.release_id)
                         OR (r.subject_kind = 'artifact' AND EXISTS (
                               SELECT 1 FROM jobs_browser_release_artifacts sibling
                                WHERE sibling.manifest_sha256 = b.manifest_sha256
                                  AND sibling.build_descriptor_sha256 =
                                      b.build_descriptor_sha256
                                  AND sibling.artifact_id = r.subject_id
                                  AND sibling.artifact_sha256 = r.subject_sha256
                             ))
                         OR (r.subject_kind = 'signing-key' AND (
                               r.subject_id = a.build_descriptor_signing_key_id
                               OR EXISTS (
                                 SELECT 1 FROM jobs_browser_release_signatures s
                                  WHERE s.key_id = r.subject_id
                                    AND s.signature_set_sha256 IN (
                                      b.manifest_signature_set_sha256,
                                      b.activation_authorization_signature_set_sha256,
                                      p.authorization_signature_set_sha256
                                    )
                               )
                             ))
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM jobs_browser_release_trust_keys k
                      WHERE k.policy_sha256 = p.policy_sha256
                        AND k.state = 'revoked'
                        AND (
                          k.key_id = a.build_descriptor_signing_key_id
                          OR EXISTS (
                            SELECT 1 FROM jobs_browser_release_signatures s
                             WHERE s.key_id = k.key_id
                               AND s.signature_set_sha256 IN (
                                 b.manifest_signature_set_sha256,
                                 b.activation_authorization_signature_set_sha256,
                                 p.authorization_signature_set_sha256
                               )
                          )
                        )
                   )
              )",
            &[&run_id, &now, &server_release_id],
        )?
        .get(0))
}

pub fn browser_release_for_claim(
    pool: &DbPool,
    account_id: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    now: i64,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let binding = sqlite_browser_release_for_claim_tx(
                &transaction,
                account_id,
                descriptor,
                server_release_id,
                now,
            )?;
            transaction.commit()?;
            Ok(binding)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let binding = postgres_browser_release_for_claim_tx(
                &mut transaction,
                account_id,
                descriptor,
                server_release_id,
                now,
            )?;
            transaction.commit()?;
            Ok(binding)
        }
    })
}

pub fn get_local_run_browser_release_binding(
    pool: &DbPool,
    account_id: &str,
    run_id: &str,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction = connection.transaction()?;
            let binding = sqlite_browser_release_binding(&transaction, run_id, account_id)?;
            transaction.commit()?;
            Ok(binding)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let binding = postgres_browser_release_binding(&mut transaction, run_id, account_id)?;
            transaction.commit()?;
            Ok(binding)
        }
    })
}

pub fn local_browser_release_availability(
    pool: &DbPool,
    account_id: &str,
    server_release_id: &str,
) -> Result<LocalBrowserReleaseAvailability> {
    local_browser_release_availability_inner(pool, account_id, server_release_id, false)
}

pub(crate) fn local_browser_release_availability_for_distribution(
    pool: &DbPool,
    account_id: &str,
    server_release_id: &str,
) -> Result<LocalBrowserReleaseAvailability> {
    local_browser_release_availability_inner(pool, account_id, server_release_id, true)
}

fn local_browser_release_availability_inner(
    pool: &DbPool,
    account_id: &str,
    server_release_id: &str,
    require_distribution_ready: bool,
) -> Result<LocalBrowserReleaseAvailability> {
    if account_id.trim().is_empty() || !browser_release_safe_id(server_release_id) {
        anyhow::bail!("invalid Browser release availability binding")
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = now_ms();
            if require_distribution_ready
                && !sqlite_runner_volume_fleet_distribution_ready(&transaction)?
            {
                transaction.commit()?;
                return Ok(browser_release_distribution_unavailable());
            }
            let availability = sqlite_local_browser_release_availability(
                &transaction,
                account_id,
                server_release_id,
                now,
            )?;
            transaction.commit()?;
            Ok(availability)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            if require_distribution_ready
                && !postgres_runner_volume_fleet_distribution_ready(&mut transaction)?
            {
                transaction.commit()?;
                return Ok(browser_release_distribution_unavailable());
            }
            postgres_lock_browser_release_registry_shared(&mut transaction)?;
            let now = local_run_claim_db_now_postgres(&mut transaction)?;
            let availability = postgres_local_browser_release_availability(
                &mut transaction,
                account_id,
                server_release_id,
                now,
            )?;
            transaction.commit()?;
            Ok(availability)
        }
    })
}

fn sqlite_local_browser_release_availability(
    connection: &rusqlite::Transaction<'_>,
    account_id: &str,
    server_release_id: &str,
    now: i64,
) -> Result<LocalBrowserReleaseAvailability> {
    let channel: Option<String> = connection
        .query_row(
            "SELECT channel FROM jobs_browser_account_channel_assignments
              WHERE account_id = ?1
              ORDER BY assignment_generation DESC LIMIT 1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(channel) = channel else {
        return Ok(browser_release_unassigned());
    };
    let activation: Option<BrowserPortalActivationRow> = connection
        .query_row(
            "SELECT a.manifest_sha256, a.accepted_server_release_ids_json,
                    a.expires_at_ms, a.manifest_signature_set_sha256,
                    a.authorization_signature_set_sha256,
                    p.authorization_signature_set_sha256, p.policy_sha256,
                    p.trust_generation, p.canonical_policy_base64url
               FROM jobs_browser_release_channel_heads h
               JOIN jobs_browser_release_activations a
                ON a.activation_sha256 = h.current_activation_sha256
                AND a.manifest_sha256 = h.current_manifest_sha256
                AND a.channel = h.channel
                AND a.trust_generation = h.current_trust_generation
                AND a.channel_sequence = h.current_channel_sequence
               JOIN jobs_browser_release_trust_policies p
                 ON p.trust_generation = a.trust_generation
                AND p.trust_generation = (
                  SELECT MAX(trust_generation)
                    FROM jobs_browser_release_trust_policies
                )
              WHERE h.channel = ?1 AND p.valid_from_ms <= ?2 AND p.expires_at_ms > ?2",
            params![channel, now],
            |row| {
                Ok(BrowserPortalActivationRow {
                    manifest_sha256: row.get(0)?,
                    accepted_server_release_ids_json: row.get(1)?,
                    expires_at_ms: row.get(2)?,
                    manifest_signature_set_sha256: row.get(3)?,
                    activation_authorization_signature_set_sha256: row.get(4)?,
                    policy_authorization_signature_set_sha256: row.get(5)?,
                    policy_sha256: row.get(6)?,
                    policy_trust_generation: row.get(7)?,
                    canonical_policy_base64url: row.get(8)?,
                })
            },
        )
        .optional()?;
    let Some(activation) = activation else {
        return Ok(browser_release_unavailable());
    };
    let Some(artifact_origin) = browser_portal_policy_artifact_origin(&activation) else {
        return Ok(browser_release_unavailable());
    };
    if activation.expires_at_ms <= now
        || !browser_activation_accepts_server(
            &activation.accepted_server_release_ids_json,
            server_release_id,
        )?
    {
        return Ok(browser_release_unavailable());
    }
    let manifest = connection
        .query_row(
            "SELECT manifest_id, release_id, release_sequence, build_id, app_version,
                    protocol_version, published_at_ms, artifact_count,
                    authorization_signature_set_sha256
               FROM jobs_browser_release_manifests WHERE manifest_sha256 = ?1",
            params![activation.manifest_sha256],
            |row| {
                Ok(BrowserPortalManifestRow {
                    manifest_id: row.get(0)?,
                    release_id: row.get(1)?,
                    release_sequence: row.get(2)?,
                    build_id: row.get(3)?,
                    app_version: row.get(4)?,
                    protocol_version: row.get(5)?,
                    published_at_ms: row.get(6)?,
                    artifact_count: row.get(7)?,
                    authorization_signature_set_sha256: row.get(8)?,
                })
            },
        )
        .optional()?;
    let Some(manifest) = manifest else {
        return Ok(browser_release_unavailable());
    };
    if manifest.authorization_signature_set_sha256 != activation.manifest_signature_set_sha256 {
        return Ok(browser_release_unavailable());
    }
    let mut statement = connection.prepare(
        "SELECT artifact.artifact_id, artifact.platform, artifact.architecture,
                artifact.package_kind, artifact.build_descriptor_sha256,
                artifact.build_descriptor_signing_key_id, artifact.artifact_url,
                artifact.artifact_filename, artifact.artifact_size_bytes,
                artifact.artifact_sha256, artifact.app_content_sha256,
                runtime.automation_bundle_sha256,
                runtime.chromium_executable_sha256
           FROM jobs_browser_release_artifacts artifact
           JOIN jobs_browser_release_artifact_runtime_components runtime
             ON runtime.manifest_sha256 = artifact.manifest_sha256
            AND runtime.artifact_id = artifact.artifact_id
            AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
            AND runtime.artifact_sha256 = artifact.artifact_sha256
            AND runtime.platform = artifact.platform
            AND runtime.architecture = artifact.architecture
            AND runtime.package_kind = artifact.package_kind
          WHERE artifact.manifest_sha256 = ?1
          ORDER BY artifact.platform, artifact.architecture,
                   artifact.package_kind, artifact.artifact_id",
    )?;
    let artifacts = statement
        .query_map(params![activation.manifest_sha256], |row| {
            Ok(BrowserPortalArtifactRow {
                artifact_id: row.get(0)?,
                platform: row.get(1)?,
                architecture: row.get(2)?,
                package_kind: row.get(3)?,
                build_descriptor_sha256: row.get(4)?,
                build_descriptor_signing_key_id: row.get(5)?,
                artifact_url: row.get(6)?,
                artifact_filename: row.get(7)?,
                artifact_size_bytes: row.get(8)?,
                artifact_sha256: row.get(9)?,
                app_content_sha256: row.get(10)?,
                automation_bundle_sha256: row.get(11)?,
                chromium_executable_sha256: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if artifacts.len() as i64 != manifest.artifact_count
        || !browser_portal_artifact_set_complete(&artifacts)
        || !browser_portal_artifacts_match_policy(
            &artifacts,
            &manifest.release_id,
            &artifact_origin,
        )
        || sqlite_portal_release_revoked(
            connection,
            &manifest.manifest_id,
            &activation.manifest_sha256,
            &manifest.release_id,
            &[
                activation.manifest_signature_set_sha256.clone(),
                activation
                    .activation_authorization_signature_set_sha256
                    .clone(),
                activation.policy_authorization_signature_set_sha256.clone(),
            ],
            &artifacts,
        )?
    {
        return Ok(browser_release_unavailable());
    }
    browser_release_available(
        channel,
        manifest.release_id,
        artifact_origin,
        activation.manifest_sha256,
        manifest.release_sequence,
        manifest.build_id,
        manifest.app_version,
        manifest.protocol_version,
        manifest.published_at_ms,
        artifacts,
    )
}

fn postgres_local_browser_release_availability(
    connection: &mut postgres::Transaction<'_>,
    account_id: &str,
    server_release_id: &str,
    now: i64,
) -> Result<LocalBrowserReleaseAvailability> {
    let channel = connection.query_opt(
        "SELECT channel FROM jobs_browser_account_channel_assignments
          WHERE account_id = $1
          ORDER BY assignment_generation DESC LIMIT 1",
        &[&account_id],
    )?;
    let Some(channel) = channel else {
        return Ok(browser_release_unassigned());
    };
    let channel: String = channel.get(0);
    let activation = connection.query_opt(
        "SELECT a.manifest_sha256, a.accepted_server_release_ids_json,
                a.expires_at_ms, a.manifest_signature_set_sha256,
                a.authorization_signature_set_sha256,
                p.authorization_signature_set_sha256, p.policy_sha256,
                p.trust_generation, p.canonical_policy_base64url
           FROM jobs_browser_release_channel_heads h
           JOIN jobs_browser_release_activations a
             ON a.activation_sha256 = h.current_activation_sha256
            AND a.manifest_sha256 = h.current_manifest_sha256
            AND a.channel = h.channel
            AND a.trust_generation = h.current_trust_generation
            AND a.channel_sequence = h.current_channel_sequence
           JOIN jobs_browser_release_trust_policies p
             ON p.trust_generation = a.trust_generation
            AND p.trust_generation = (
              SELECT MAX(trust_generation)
                FROM jobs_browser_release_trust_policies
            )
          WHERE h.channel = $1 AND p.valid_from_ms <= $2 AND p.expires_at_ms > $2",
        &[&channel, &now],
    )?;
    let Some(activation) = activation else {
        return Ok(browser_release_unavailable());
    };
    let activation = BrowserPortalActivationRow {
        manifest_sha256: activation.get(0),
        accepted_server_release_ids_json: activation.get(1),
        expires_at_ms: activation.get(2),
        manifest_signature_set_sha256: activation.get(3),
        activation_authorization_signature_set_sha256: activation.get(4),
        policy_authorization_signature_set_sha256: activation.get(5),
        policy_sha256: activation.get(6),
        policy_trust_generation: activation.get(7),
        canonical_policy_base64url: activation.get(8),
    };
    let Some(artifact_origin) = browser_portal_policy_artifact_origin(&activation) else {
        return Ok(browser_release_unavailable());
    };
    if activation.expires_at_ms <= now
        || !browser_activation_accepts_server(
            &activation.accepted_server_release_ids_json,
            server_release_id,
        )?
    {
        return Ok(browser_release_unavailable());
    }
    let manifest = connection.query_opt(
        "SELECT manifest_id, release_id, release_sequence, build_id, app_version,
                protocol_version, published_at_ms, artifact_count,
                authorization_signature_set_sha256
           FROM jobs_browser_release_manifests WHERE manifest_sha256 = $1",
        &[&activation.manifest_sha256],
    )?;
    let Some(manifest) = manifest else {
        return Ok(browser_release_unavailable());
    };
    let manifest_id: String = manifest.get(0);
    let release_id: String = manifest.get(1);
    let release_sequence: i64 = manifest.get(2);
    let build_id: String = manifest.get(3);
    let app_version: String = manifest.get(4);
    let protocol_version: i64 = manifest.get(5);
    let published_at_ms: i64 = manifest.get(6);
    let artifact_count: i64 = manifest.get(7);
    let manifest_authorization_signature_set_sha256: String = manifest.get(8);
    if manifest_authorization_signature_set_sha256 != activation.manifest_signature_set_sha256 {
        return Ok(browser_release_unavailable());
    }
    let artifacts = connection
        .query(
            "SELECT artifact.artifact_id, artifact.platform, artifact.architecture,
                    artifact.package_kind, artifact.build_descriptor_sha256,
                    artifact.build_descriptor_signing_key_id, artifact.artifact_url,
                    artifact.artifact_filename, artifact.artifact_size_bytes,
                    artifact.artifact_sha256, artifact.app_content_sha256,
                    runtime.automation_bundle_sha256,
                    runtime.chromium_executable_sha256
               FROM jobs_browser_release_artifacts artifact
               JOIN jobs_browser_release_artifact_runtime_components runtime
                 ON runtime.manifest_sha256 = artifact.manifest_sha256
                AND runtime.artifact_id = artifact.artifact_id
                AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
                AND runtime.artifact_sha256 = artifact.artifact_sha256
                AND runtime.platform = artifact.platform
                AND runtime.architecture = artifact.architecture
                AND runtime.package_kind = artifact.package_kind
              WHERE artifact.manifest_sha256 = $1
              ORDER BY artifact.platform, artifact.architecture,
                       artifact.package_kind, artifact.artifact_id",
            &[&activation.manifest_sha256],
        )?
        .into_iter()
        .map(|row| BrowserPortalArtifactRow {
            artifact_id: row.get(0),
            platform: row.get(1),
            architecture: row.get(2),
            package_kind: row.get(3),
            build_descriptor_sha256: row.get(4),
            build_descriptor_signing_key_id: row.get(5),
            artifact_url: row.get(6),
            artifact_filename: row.get(7),
            artifact_size_bytes: row.get(8),
            artifact_sha256: row.get(9),
            app_content_sha256: row.get(10),
            automation_bundle_sha256: row.get(11),
            chromium_executable_sha256: row.get(12),
        })
        .collect::<Vec<_>>();
    if artifacts.len() as i64 != artifact_count
        || !browser_portal_artifact_set_complete(&artifacts)
        || !browser_portal_artifacts_match_policy(&artifacts, &release_id, &artifact_origin)
        || postgres_portal_release_revoked(
            connection,
            &manifest_id,
            &activation.manifest_sha256,
            &release_id,
            &[
                activation.manifest_signature_set_sha256.clone(),
                activation
                    .activation_authorization_signature_set_sha256
                    .clone(),
                activation.policy_authorization_signature_set_sha256.clone(),
            ],
            &artifacts,
        )?
    {
        return Ok(browser_release_unavailable());
    }
    browser_release_available(
        channel,
        release_id,
        artifact_origin,
        activation.manifest_sha256,
        release_sequence,
        build_id,
        app_version,
        protocol_version,
        published_at_ms,
        artifacts,
    )
}

fn browser_portal_policy_artifact_origin(
    activation: &BrowserPortalActivationRow,
) -> Option<String> {
    let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&activation.canonical_policy_base64url)
        .ok()?;
    let policy = parse_canonical_browser_release_trust_policy(&canonical).ok()?;
    if browser_release_authority_sha256(&canonical) != activation.policy_sha256
        || policy.trust_generation != activation.policy_trust_generation
    {
        return None;
    }
    Some(policy.artifact_origin)
}

fn browser_portal_artifacts_match_policy(
    artifacts: &[BrowserPortalArtifactRow],
    release_id: &str,
    artifact_origin: &str,
) -> bool {
    artifacts.iter().all(|artifact| {
        browser_release_immutable_artifact_url(&artifact.artifact_url, release_id)
            && browser_release_artifact_url_matches_package_kind(
                &artifact.artifact_url,
                &artifact.package_kind,
            )
            && reqwest::Url::parse(&artifact.artifact_url).is_ok_and(|url| {
                url.origin().ascii_serialization() == artifact_origin
                    && url
                        .path_segments()
                        .and_then(|mut segments| segments.next_back())
                        == Some(artifact.artifact_filename.as_str())
            })
    })
}

fn browser_portal_artifact_set_complete(artifacts: &[BrowserPortalArtifactRow]) -> bool {
    let actual = artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.platform.clone(),
                artifact.architecture.clone(),
                artifact.package_kind.clone(),
                artifact.build_descriptor_sha256.clone(),
                artifact.app_content_sha256.clone(),
                artifact.automation_bundle_sha256.clone(),
                artifact.chromium_executable_sha256.clone(),
                artifact.artifact_url.clone(),
            )
        })
        .collect::<Vec<_>>();
    browser_artifact_contract_complete(&actual)
}

fn browser_artifact_contract_complete(actual: &[BrowserReleaseArtifactContractRow]) -> bool {
    let identities = actual
        .iter()
        .map(|(platform, architecture, package_kind, _, _, _, _, _)| {
            format!("{platform}/{architecture}/{package_kind}")
        })
        .collect::<BTreeSet<_>>();
    let descriptors = actual.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut grouped, (platform, architecture, _, descriptor_sha256, _, _, _, _)| {
            grouped
                .entry(format!("{platform}/{architecture}"))
                .or_default()
                .insert(descriptor_sha256.clone());
            grouped
        },
    );
    let descriptor_digests = descriptors
        .values()
        .flat_map(|values| values.iter().cloned())
        .collect::<BTreeSet<_>>();
    let artifact_urls = actual
        .iter()
        .map(|(_, _, _, _, _, _, _, artifact_url)| artifact_url)
        .collect::<BTreeSet<_>>();
    let app_content = actual.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut grouped, (platform, architecture, _, _, app_content_sha256, _, _, _)| {
            grouped
                .entry(format!("{platform}/{architecture}"))
                .or_default()
                .insert(app_content_sha256.clone());
            grouped
        },
    );
    let automation_bundles = actual.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut grouped, (platform, architecture, _, _, _, automation_sha256, _, _)| {
            grouped
                .entry(format!("{platform}/{architecture}"))
                .or_default()
                .insert(automation_sha256.clone());
            grouped
        },
    );
    let chromium_executables = actual.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut grouped, (platform, architecture, _, _, _, _, chromium_sha256, _)| {
            grouped
                .entry(format!("{platform}/{architecture}"))
                .or_default()
                .insert(chromium_sha256.clone());
            grouped
        },
    );
    identities.len() == actual.len()
        && artifact_urls.len() == actual.len()
        && actual.iter().all(
            |(
                _,
                _,
                package_kind,
                descriptor_sha256,
                app_content_sha256,
                automation_bundle_sha256,
                chromium_executable_sha256,
                artifact_url,
            )| {
                [
                    descriptor_sha256,
                    app_content_sha256,
                    automation_bundle_sha256,
                    chromium_executable_sha256,
                ]
                .iter()
                .all(|digest| {
                    digest.len() == 64
                        && **digest == digest.to_ascii_lowercase()
                        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                }) && browser_release_artifact_url_matches_package_kind(artifact_url, package_kind)
            },
        )
        && app_content.len() == 3
        && app_content.values().all(|values| values.len() == 1)
        && automation_bundles.len() == 3
        && automation_bundles.values().all(|values| values.len() == 1)
        && chromium_executables.len() == 3
        && chromium_executables
            .values()
            .all(|values| values.len() == 1)
        && descriptors.len() == 3
        && descriptors.values().all(|values| values.len() == 1)
        && descriptor_digests.len() == 3
        && identities
            == [
                "darwin/arm64/darwin-dmg".to_string(),
                "darwin/arm64/darwin-zip".to_string(),
                "darwin/x64/darwin-dmg".to_string(),
                "darwin/x64/darwin-zip".to_string(),
                "windows/x64/windows-nsis".to_string(),
            ]
            .into_iter()
            .collect()
}

fn sqlite_portal_release_revoked(
    connection: &rusqlite::Transaction<'_>,
    manifest_id: &str,
    manifest_sha256: &str,
    release_id: &str,
    signature_sets: &[String; 3],
    artifacts: &[BrowserPortalArtifactRow],
) -> Result<bool> {
    let release_sha256 = hex::encode(Sha256::digest(release_id.as_bytes()));
    let global_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM jobs_browser_release_revocations r
          WHERE (r.subject_kind = 'manifest' AND r.subject_id = ?1
                 AND r.subject_sha256 = ?2)
             OR (r.subject_kind = 'release' AND r.subject_id = ?3
                 AND r.subject_sha256 = ?4)
             OR (r.subject_kind = 'signing-key' AND EXISTS (
                   SELECT 1 FROM jobs_browser_release_signatures s
                    WHERE s.key_id = r.subject_id
                      AND s.signature_set_sha256 IN (?5, ?6, ?7)
                 ))",
        params![
            manifest_id,
            manifest_sha256,
            release_id,
            release_sha256,
            signature_sets[0],
            signature_sets[1],
            signature_sets[2],
        ],
        |row| row.get(0),
    )?;
    if global_count > 0 {
        return Ok(true);
    }
    let revoked_authority_key_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM jobs_browser_release_trust_keys k
          WHERE k.trust_generation = (
                  SELECT MAX(trust_generation)
                    FROM jobs_browser_release_trust_policies
                )
            AND k.state = 'revoked'
            AND EXISTS (
              SELECT 1 FROM jobs_browser_release_signatures s
               WHERE s.key_id = k.key_id
                 AND s.signature_set_sha256 IN (?1, ?2, ?3)
            )",
        params![signature_sets[0], signature_sets[1], signature_sets[2]],
        |row| row.get(0),
    )?;
    if revoked_authority_key_count > 0 {
        return Ok(true);
    }
    for artifact in artifacts {
        let build_key_revoked: i64 = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_browser_release_trust_keys k
                 WHERE k.trust_generation = (
                         SELECT MAX(trust_generation)
                           FROM jobs_browser_release_trust_policies
                       )
                   AND k.key_id = ?1 AND k.state = 'revoked'
              )",
            params![artifact.build_descriptor_signing_key_id],
            |row| row.get(0),
        )?;
        if build_key_revoked != 0 {
            return Ok(true);
        }
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM jobs_browser_release_revocations
              WHERE (subject_kind = 'artifact' AND subject_id = ?1
                     AND subject_sha256 = ?2)
                 OR (subject_kind = 'build-descriptor' AND subject_id = ?3
                     AND subject_sha256 = ?3)
                 OR (subject_kind = 'signing-key' AND subject_id = ?4)",
            params![
                artifact.artifact_id,
                artifact.artifact_sha256,
                artifact.build_descriptor_sha256,
                artifact.build_descriptor_signing_key_id,
            ],
            |row| row.get(0),
        )?;
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn postgres_portal_release_revoked(
    connection: &mut postgres::Transaction<'_>,
    manifest_id: &str,
    manifest_sha256: &str,
    release_id: &str,
    signature_sets: &[String; 3],
    artifacts: &[BrowserPortalArtifactRow],
) -> Result<bool> {
    let release_sha256 = hex::encode(Sha256::digest(release_id.as_bytes()));
    let global_count: i64 = connection
        .query_one(
            "SELECT COUNT(*) FROM jobs_browser_release_revocations r
              WHERE (r.subject_kind = 'manifest' AND r.subject_id = $1
                     AND r.subject_sha256 = $2)
                 OR (r.subject_kind = 'release' AND r.subject_id = $3
                     AND r.subject_sha256 = $4)
                 OR (r.subject_kind = 'signing-key' AND EXISTS (
                       SELECT 1 FROM jobs_browser_release_signatures s
                        WHERE s.key_id = r.subject_id
                          AND s.signature_set_sha256 IN ($5, $6, $7)
                     ))",
            &[
                &manifest_id,
                &manifest_sha256,
                &release_id,
                &release_sha256,
                &signature_sets[0],
                &signature_sets[1],
                &signature_sets[2],
            ],
        )?
        .get(0);
    if global_count > 0 {
        return Ok(true);
    }
    let revoked_authority_key_count: i64 = connection
        .query_one(
            "SELECT COUNT(*) FROM jobs_browser_release_trust_keys k
              WHERE k.trust_generation = (
                      SELECT MAX(trust_generation)
                        FROM jobs_browser_release_trust_policies
                    )
                AND k.state = 'revoked'
                AND EXISTS (
                  SELECT 1 FROM jobs_browser_release_signatures s
                   WHERE s.key_id = k.key_id
                     AND s.signature_set_sha256 IN ($1, $2, $3)
                )",
            &[&signature_sets[0], &signature_sets[1], &signature_sets[2]],
        )?
        .get(0);
    if revoked_authority_key_count > 0 {
        return Ok(true);
    }
    for artifact in artifacts {
        let build_key_revoked: bool = connection
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_browser_release_trust_keys k
                     WHERE k.trust_generation = (
                             SELECT MAX(trust_generation)
                               FROM jobs_browser_release_trust_policies
                           )
                       AND k.key_id = $1 AND k.state = 'revoked'
                  )",
                &[&artifact.build_descriptor_signing_key_id],
            )?
            .get(0);
        if build_key_revoked {
            return Ok(true);
        }
        let count: i64 = connection
            .query_one(
                "SELECT COUNT(*) FROM jobs_browser_release_revocations
                  WHERE (subject_kind = 'artifact' AND subject_id = $1
                         AND subject_sha256 = $2)
                     OR (subject_kind = 'build-descriptor' AND subject_id = $3
                         AND subject_sha256 = $3)
                     OR (subject_kind = 'signing-key' AND subject_id = $4)",
                &[
                    &artifact.artifact_id,
                    &artifact.artifact_sha256,
                    &artifact.build_descriptor_sha256,
                    &artifact.build_descriptor_signing_key_id,
                ],
            )?
            .get(0);
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn browser_release_available(
    channel: String,
    release_id: String,
    artifact_origin: String,
    manifest_sha256: String,
    release_sequence: i64,
    build_id: String,
    app_version: String,
    protocol_version: i64,
    published_at_ms: i64,
    artifacts: Vec<BrowserPortalArtifactRow>,
) -> Result<LocalBrowserReleaseAvailability> {
    let artifacts = artifacts
        .into_iter()
        .map(|artifact| {
            let (package_kind, role) = match artifact.package_kind.as_str() {
                "darwin-dmg" => ("dmg", "installer"),
                "darwin-zip" => ("zip", "updater"),
                "windows-nsis" => ("exe", "installer"),
                _ => anyhow::bail!("invalid Browser portal artifact package kind"),
            };
            Ok(LocalBrowserReleaseArtifact {
                platform: if artifact.platform == "darwin" {
                    "macos".to_string()
                } else {
                    artifact.platform
                },
                architecture: artifact.architecture,
                package_kind: package_kind.to_string(),
                role: role.to_string(),
                file_name: artifact.artifact_filename,
                url: artifact.artifact_url,
                size_bytes: artifact.artifact_size_bytes,
                sha256: artifact.artifact_sha256,
                descriptor_sha256: artifact.build_descriptor_sha256,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LocalBrowserReleaseAvailability::Available {
        reason: format!("The active {channel} release is available for this account."),
        channel,
        release_id,
        artifact_origin,
        manifest_sha256,
        release_sequence,
        build_id,
        app_version,
        protocol_version,
        released_at_ms: published_at_ms,
        artifacts,
    })
}

fn browser_release_unassigned() -> LocalBrowserReleaseAvailability {
    LocalBrowserReleaseAvailability::Unassigned {
        reason: "This account has not been assigned a Bluey Browser release channel.".to_string(),
    }
}

fn browser_release_unavailable() -> LocalBrowserReleaseAvailability {
    LocalBrowserReleaseAvailability::Unavailable {
        reason: "No active, compatible, non-revoked Bluey Browser release is available for this account."
            .to_string(),
    }
}

fn browser_release_distribution_unavailable() -> LocalBrowserReleaseAvailability {
    LocalBrowserReleaseAvailability::Disabled {
        reason: "Bluey Browser distribution is disabled for this release.".to_string(),
    }
}

fn sqlite_browser_release_for_claim_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    now: i64,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    let assignment: Option<(String, i64, String)> = tx
        .query_row(
            "SELECT assignment_sha256, assignment_generation, channel
               FROM jobs_browser_account_channel_assignments
              WHERE account_id = ?1
              ORDER BY assignment_generation DESC
              LIMIT 1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((assignment_sha256, assignment_generation, channel)) = assignment else {
        return Ok(None);
    };
    let activation = tx
        .query_row(
            "SELECT h.head_revision, h.current_transition_sha256,
                    a.activation_sha256, a.activation_generation, a.trust_generation,
                    a.channel_sequence, p.policy_sha256, p.canonical_policy_base64url,
                    a.manifest_signature_set_sha256,
                    a.authorization_signature_set_sha256, a.manifest_sha256,
                    a.accepted_server_release_ids_json, a.expires_at_ms
               FROM jobs_browser_release_channel_heads h
               JOIN jobs_browser_release_activations a
                 ON a.activation_sha256 = h.current_activation_sha256
                AND a.manifest_sha256 = h.current_manifest_sha256
                AND a.channel = h.channel
                AND a.trust_generation = h.current_trust_generation
                AND a.channel_sequence = h.current_channel_sequence
               JOIN jobs_browser_release_trust_policies p
                 ON p.trust_generation = a.trust_generation
                AND p.trust_generation = (
                  SELECT MAX(trust_generation)
                    FROM jobs_browser_release_trust_policies
                )
              WHERE h.channel = ?1 AND p.valid_from_ms <= ?2 AND p.expires_at_ms > ?2",
            params![channel, now],
            |row| {
                Ok(BrowserReleaseActivationRow {
                    channel_head_revision: row.get(0)?,
                    channel_transition_sha256: row.get(1)?,
                    activation_sha256: row.get(2)?,
                    activation_generation: row.get(3)?,
                    trust_generation: row.get(4)?,
                    channel_sequence: row.get(5)?,
                    trust_policy_sha256: row.get(6)?,
                    canonical_policy_base64url: row.get(7)?,
                    manifest_signature_set_sha256: row.get(8)?,
                    authorization_signature_set_sha256: row.get(9)?,
                    manifest_sha256: row.get(10)?,
                    accepted_server_release_ids_json: row.get(11)?,
                    expires_at_ms: row.get(12)?,
                })
            },
        )
        .optional()?;
    let Some(activation) = activation else {
        return Ok(None);
    };
    let Some(artifact_origin) = browser_release_claim_policy_artifact_origin(&activation) else {
        return Ok(None);
    };
    if activation.expires_at_ms <= now
        || !browser_activation_accepts_server(
            &activation.accepted_server_release_ids_json,
            server_release_id,
        )?
    {
        return Ok(None);
    }
    let manifest = tx
        .query_row(
            "SELECT manifest_id, release_id, release_sequence,
                    authorization_signature_set_sha256, published_at_ms, artifact_count
               FROM jobs_browser_release_manifests
              WHERE manifest_sha256 = ?1 AND release_id = ?2 AND build_id = ?3
                AND app_version = ?4 AND protocol_version = ?5 AND source_commit = ?6
                AND electron_version = ?7 AND playwright_version = ?8
                AND chromium_revision = ?9",
            params![
                activation.manifest_sha256,
                descriptor.release_id,
                descriptor.build_id,
                descriptor.app_version,
                descriptor.protocol_version,
                descriptor.source_commit,
                descriptor.electron_version,
                descriptor.playwright_version,
                descriptor.chromium_revision,
            ],
            |row| {
                Ok(BrowserReleaseManifestRow {
                    manifest_id: row.get(0)?,
                    release_id: row.get(1)?,
                    release_sequence: row.get(2)?,
                    authorization_signature_set_sha256: row.get(3)?,
                    published_at_ms: row.get(4)?,
                    artifact_count: row.get(5)?,
                })
            },
        )
        .optional()?;
    let Some(manifest) = manifest else {
        return Ok(None);
    };
    if manifest.authorization_signature_set_sha256 != activation.manifest_signature_set_sha256 {
        return Ok(None);
    }
    let package_kind = browser_installer_package_kind(&descriptor.platform);
    let artifact = tx
        .query_row(
            "SELECT artifact.artifact_id, artifact.artifact_sha256,
                    artifact.artifact_url, artifact.artifact_filename,
                    artifact.artifact_size_bytes, artifact.package_kind,
                    runtime.automation_bundle_sha256,
                    runtime.chromium_executable_sha256
               FROM jobs_browser_release_artifacts artifact
               JOIN jobs_browser_release_artifact_runtime_components runtime
                 ON runtime.manifest_sha256 = artifact.manifest_sha256
                AND runtime.artifact_id = artifact.artifact_id
                AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
                AND runtime.artifact_sha256 = artifact.artifact_sha256
                AND runtime.platform = artifact.platform
                AND runtime.architecture = artifact.architecture
                AND runtime.package_kind = artifact.package_kind
              WHERE artifact.manifest_sha256 = ?1 AND artifact.platform = ?2
                AND artifact.architecture = ?3 AND artifact.package_kind = ?4
                AND artifact.build_descriptor_sha256 = ?5
                AND artifact.build_descriptor_base64url = ?6
                AND artifact.build_descriptor_signature_base64url = ?7
                AND artifact.build_descriptor_signing_key_id = ?8",
            params![
                activation.manifest_sha256,
                descriptor.platform,
                descriptor.architecture,
                package_kind,
                descriptor.descriptor_sha256,
                descriptor.descriptor_base64url,
                descriptor.signature_base64url,
                descriptor.signing_key_id,
            ],
            |row| {
                Ok(BrowserReleaseArtifactRow {
                    artifact_id: row.get(0)?,
                    artifact_sha256: row.get(1)?,
                    artifact_url: row.get(2)?,
                    artifact_filename: row.get(3)?,
                    artifact_size_bytes: row.get(4)?,
                    package_kind: row.get(5)?,
                    automation_bundle_sha256: row.get(6)?,
                    chromium_executable_sha256: row.get(7)?,
                })
            },
        )
        .optional()?;
    let Some(artifact) = artifact else {
        return Ok(None);
    };
    let actual_artifact_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_browser_release_artifacts WHERE manifest_sha256 = ?1",
        params![activation.manifest_sha256],
        |row| row.get(0),
    )?;
    let mut target_statement = tx.prepare(
        "SELECT artifact.platform, artifact.architecture, artifact.package_kind,
                artifact.build_descriptor_sha256, artifact.app_content_sha256,
                runtime.automation_bundle_sha256,
                runtime.chromium_executable_sha256, artifact.artifact_url
           FROM jobs_browser_release_artifacts artifact
           JOIN jobs_browser_release_artifact_runtime_components runtime
             ON runtime.manifest_sha256 = artifact.manifest_sha256
            AND runtime.artifact_id = artifact.artifact_id
            AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
            AND runtime.artifact_sha256 = artifact.artifact_sha256
            AND runtime.platform = artifact.platform
            AND runtime.architecture = artifact.architecture
            AND runtime.package_kind = artifact.package_kind
          WHERE artifact.manifest_sha256 = ?1
          ORDER BY artifact.platform, artifact.architecture, artifact.package_kind",
    )?;
    let artifact_targets = target_statement
        .query_map(params![activation.manifest_sha256], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if actual_artifact_count != manifest.artifact_count
        || !browser_release_claim_artifact_set_matches_policy(
            &artifact_targets,
            &manifest.release_id,
            &artifact_origin,
        )
        || sqlite_browser_release_is_revoked(tx, descriptor, &activation, &manifest)?
    {
        return Ok(None);
    }
    Ok(Some(browser_release_claim_binding(
        assignment_sha256,
        assignment_generation,
        channel,
        activation,
        manifest,
        artifact,
        descriptor,
    )))
}

fn postgres_browser_release_for_claim_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    descriptor: &VerifiedBrowserBuildDescriptor,
    server_release_id: &str,
    now: i64,
) -> Result<Option<BrowserReleaseClaimBinding>> {
    let assignment = tx.query_opt(
        "SELECT assignment_sha256, assignment_generation, channel
           FROM jobs_browser_account_channel_assignments
          WHERE account_id = $1
          ORDER BY assignment_generation DESC
          LIMIT 1
          FOR UPDATE",
        &[&account_id],
    )?;
    let Some(assignment) = assignment else {
        return Ok(None);
    };
    let assignment_sha256: String = assignment.get(0);
    let assignment_generation: i64 = assignment.get(1);
    let channel: String = assignment.get(2);
    let advisory_lock = format!("jobs-browser-release-channel:{channel}");
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&advisory_lock],
    )?;
    let activation = tx.query_opt(
        "SELECT h.head_revision, h.current_transition_sha256,
                a.activation_sha256, a.activation_generation, a.trust_generation,
                a.channel_sequence, p.policy_sha256, p.canonical_policy_base64url,
                a.manifest_signature_set_sha256,
                a.authorization_signature_set_sha256, a.manifest_sha256,
                a.accepted_server_release_ids_json, a.expires_at_ms
           FROM jobs_browser_release_channel_heads h
           JOIN jobs_browser_release_activations a
             ON a.activation_sha256 = h.current_activation_sha256
            AND a.manifest_sha256 = h.current_manifest_sha256
            AND a.channel = h.channel
            AND a.trust_generation = h.current_trust_generation
            AND a.channel_sequence = h.current_channel_sequence
           JOIN jobs_browser_release_trust_policies p
             ON p.trust_generation = a.trust_generation
            AND p.trust_generation = (
              SELECT MAX(trust_generation)
                FROM jobs_browser_release_trust_policies
            )
          WHERE h.channel = $1 AND p.valid_from_ms <= $2 AND p.expires_at_ms > $2
          FOR UPDATE OF h",
        &[&channel, &now],
    )?;
    let Some(activation) = activation else {
        return Ok(None);
    };
    let activation = BrowserReleaseActivationRow {
        channel_head_revision: activation.get(0),
        channel_transition_sha256: activation.get(1),
        activation_sha256: activation.get(2),
        activation_generation: activation.get(3),
        trust_generation: activation.get(4),
        channel_sequence: activation.get(5),
        trust_policy_sha256: activation.get(6),
        canonical_policy_base64url: activation.get(7),
        manifest_signature_set_sha256: activation.get(8),
        authorization_signature_set_sha256: activation.get(9),
        manifest_sha256: activation.get(10),
        accepted_server_release_ids_json: activation.get(11),
        expires_at_ms: activation.get(12),
    };
    let Some(artifact_origin) = browser_release_claim_policy_artifact_origin(&activation) else {
        return Ok(None);
    };
    if activation.expires_at_ms <= now
        || !browser_activation_accepts_server(
            &activation.accepted_server_release_ids_json,
            server_release_id,
        )?
    {
        return Ok(None);
    }
    let manifest = tx.query_opt(
        "SELECT manifest_id, release_id, release_sequence,
                authorization_signature_set_sha256, published_at_ms, artifact_count
           FROM jobs_browser_release_manifests
          WHERE manifest_sha256 = $1 AND release_id = $2 AND build_id = $3
            AND app_version = $4 AND protocol_version = $5 AND source_commit = $6
            AND electron_version = $7 AND playwright_version = $8
            AND chromium_revision = $9",
        &[
            &activation.manifest_sha256,
            &descriptor.release_id,
            &descriptor.build_id,
            &descriptor.app_version,
            &descriptor.protocol_version,
            &descriptor.source_commit,
            &descriptor.electron_version,
            &descriptor.playwright_version,
            &descriptor.chromium_revision,
        ],
    )?;
    let Some(manifest) = manifest else {
        return Ok(None);
    };
    let manifest = BrowserReleaseManifestRow {
        manifest_id: manifest.get(0),
        release_id: manifest.get(1),
        release_sequence: manifest.get(2),
        authorization_signature_set_sha256: manifest.get(3),
        published_at_ms: manifest.get(4),
        artifact_count: manifest.get(5),
    };
    if manifest.authorization_signature_set_sha256 != activation.manifest_signature_set_sha256 {
        return Ok(None);
    }
    let package_kind = browser_installer_package_kind(&descriptor.platform);
    let artifact = tx.query_opt(
        "SELECT artifact.artifact_id, artifact.artifact_sha256,
                artifact.artifact_url, artifact.artifact_filename,
                artifact.artifact_size_bytes, artifact.package_kind,
                runtime.automation_bundle_sha256,
                runtime.chromium_executable_sha256
           FROM jobs_browser_release_artifacts artifact
           JOIN jobs_browser_release_artifact_runtime_components runtime
             ON runtime.manifest_sha256 = artifact.manifest_sha256
            AND runtime.artifact_id = artifact.artifact_id
            AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
            AND runtime.artifact_sha256 = artifact.artifact_sha256
            AND runtime.platform = artifact.platform
            AND runtime.architecture = artifact.architecture
            AND runtime.package_kind = artifact.package_kind
          WHERE artifact.manifest_sha256 = $1 AND artifact.platform = $2
            AND artifact.architecture = $3 AND artifact.package_kind = $4
            AND artifact.build_descriptor_sha256 = $5
            AND artifact.build_descriptor_base64url = $6
            AND artifact.build_descriptor_signature_base64url = $7
            AND artifact.build_descriptor_signing_key_id = $8",
        &[
            &activation.manifest_sha256,
            &descriptor.platform,
            &descriptor.architecture,
            &package_kind,
            &descriptor.descriptor_sha256,
            &descriptor.descriptor_base64url,
            &descriptor.signature_base64url,
            &descriptor.signing_key_id,
        ],
    )?;
    let Some(artifact) = artifact else {
        return Ok(None);
    };
    let artifact = BrowserReleaseArtifactRow {
        artifact_id: artifact.get(0),
        artifact_sha256: artifact.get(1),
        artifact_url: artifact.get(2),
        artifact_filename: artifact.get(3),
        artifact_size_bytes: artifact.get(4),
        package_kind: artifact.get(5),
        automation_bundle_sha256: artifact.get(6),
        chromium_executable_sha256: artifact.get(7),
    };
    let actual_artifact_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_browser_release_artifacts WHERE manifest_sha256 = $1",
            &[&activation.manifest_sha256],
        )?
        .get(0);
    let artifact_targets = tx
        .query(
            "SELECT artifact.platform, artifact.architecture,
                    artifact.package_kind, artifact.build_descriptor_sha256,
                    artifact.app_content_sha256, runtime.automation_bundle_sha256,
                    runtime.chromium_executable_sha256, artifact.artifact_url
               FROM jobs_browser_release_artifacts artifact
               JOIN jobs_browser_release_artifact_runtime_components runtime
                 ON runtime.manifest_sha256 = artifact.manifest_sha256
                AND runtime.artifact_id = artifact.artifact_id
                AND runtime.build_descriptor_sha256 = artifact.build_descriptor_sha256
                AND runtime.artifact_sha256 = artifact.artifact_sha256
                AND runtime.platform = artifact.platform
                AND runtime.architecture = artifact.architecture
                AND runtime.package_kind = artifact.package_kind
              WHERE artifact.manifest_sha256 = $1
              ORDER BY artifact.platform, artifact.architecture,
                       artifact.package_kind",
            &[&activation.manifest_sha256],
        )?
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
                row.get::<_, String>(3),
                row.get::<_, String>(4),
                row.get::<_, String>(5),
                row.get::<_, String>(6),
                row.get::<_, String>(7),
            )
        })
        .collect::<Vec<_>>();
    if actual_artifact_count != manifest.artifact_count
        || !browser_release_claim_artifact_set_matches_policy(
            &artifact_targets,
            &manifest.release_id,
            &artifact_origin,
        )
        || postgres_browser_release_is_revoked(tx, descriptor, &activation, &manifest)?
    {
        return Ok(None);
    }
    Ok(Some(browser_release_claim_binding(
        assignment_sha256,
        assignment_generation,
        channel,
        activation,
        manifest,
        artifact,
        descriptor,
    )))
}

fn browser_activation_accepts_server(accepted_json: &str, server_release_id: &str) -> Result<bool> {
    let accepted: Vec<String> = serde_json::from_str(accepted_json)
        .context("parse Browser activation server release allowlist")?;
    if accepted.is_empty()
        || accepted.len() > 32
        || accepted.iter().any(|value| !browser_release_safe_id(value))
        || accepted
            .windows(2)
            .any(|window| window[0].as_str() >= window[1].as_str())
    {
        anyhow::bail!("invalid Browser activation server release allowlist")
    }
    Ok(accepted.iter().any(|value| value == server_release_id))
}

fn browser_release_claim_policy_artifact_origin(
    activation: &BrowserReleaseActivationRow,
) -> Option<String> {
    let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&activation.canonical_policy_base64url)
        .ok()?;
    let policy = parse_canonical_browser_release_trust_policy(&canonical).ok()?;
    if browser_release_authority_sha256(&canonical) != activation.trust_policy_sha256
        || policy.trust_generation != activation.trust_generation
    {
        return None;
    }
    Some(policy.artifact_origin)
}

fn browser_release_claim_artifact_set_matches_policy(
    artifacts: &[BrowserReleaseArtifactContractRow],
    release_id: &str,
    artifact_origin: &str,
) -> bool {
    browser_artifact_contract_complete(artifacts)
        && artifacts.iter().all(|(_, _, _, _, _, _, _, artifact_url)| {
            browser_release_immutable_artifact_url(artifact_url, release_id)
                && reqwest::Url::parse(artifact_url)
                    .is_ok_and(|url| url.origin().ascii_serialization() == artifact_origin)
        })
}

fn browser_installer_package_kind(platform: &str) -> &'static str {
    if platform == "darwin" {
        "darwin-dmg"
    } else {
        "windows-nsis"
    }
}

fn sqlite_browser_release_is_revoked(
    tx: &rusqlite::Transaction<'_>,
    descriptor: &VerifiedBrowserBuildDescriptor,
    activation: &BrowserReleaseActivationRow,
    manifest: &BrowserReleaseManifestRow,
) -> Result<bool> {
    let release_sha256 = hex::encode(Sha256::digest(manifest.release_id.as_bytes()));
    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_browser_release_revocations r
          WHERE (r.subject_kind = 'build-descriptor' AND r.subject_id = ?1
                 AND r.subject_sha256 = ?1)
             OR (r.subject_kind = 'manifest' AND r.subject_id = ?2
                 AND r.subject_sha256 = ?3)
             OR (r.subject_kind = 'release' AND r.subject_id = ?4
                 AND r.subject_sha256 = ?5)
             OR (r.subject_kind = 'artifact' AND EXISTS (
                   SELECT 1 FROM jobs_browser_release_artifacts sibling
                    WHERE sibling.manifest_sha256 = ?3
                      AND sibling.build_descriptor_sha256 = ?1
                      AND sibling.artifact_id = r.subject_id
                      AND sibling.artifact_sha256 = r.subject_sha256
                 ))
             OR (r.subject_kind = 'signing-key' AND (
                   r.subject_id = ?6 OR EXISTS (
                     SELECT 1 FROM jobs_browser_release_signatures s
                      WHERE s.key_id = r.subject_id
                        AND s.signature_set_sha256 IN (
                          ?7, ?8,
                          (SELECT authorization_signature_set_sha256
                             FROM jobs_browser_release_trust_policies
                            WHERE policy_sha256 = ?9)
                        )
                   )
                 ))",
        params![
            descriptor.descriptor_sha256,
            manifest.manifest_id,
            activation.manifest_sha256,
            manifest.release_id,
            release_sha256,
            descriptor.signing_key_id,
            activation.manifest_signature_set_sha256,
            activation.authorization_signature_set_sha256,
            activation.trust_policy_sha256,
        ],
        |row| row.get(0),
    )?;
    if count > 0 {
        return Ok(true);
    }
    let revoked_key_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_browser_release_trust_keys k
          WHERE k.policy_sha256 = ?1 AND k.state = 'revoked'
            AND (
              k.key_id = ?2 OR EXISTS (
                SELECT 1 FROM jobs_browser_release_signatures s
                 WHERE s.key_id = k.key_id
                   AND s.signature_set_sha256 IN (
                     ?3, ?4,
                     (SELECT authorization_signature_set_sha256
                        FROM jobs_browser_release_trust_policies
                       WHERE policy_sha256 = ?1)
                   )
              )
            )",
        params![
            activation.trust_policy_sha256,
            descriptor.signing_key_id,
            activation.manifest_signature_set_sha256,
            activation.authorization_signature_set_sha256,
        ],
        |row| row.get(0),
    )?;
    Ok(revoked_key_count > 0)
}

fn postgres_browser_release_is_revoked(
    tx: &mut postgres::Transaction<'_>,
    descriptor: &VerifiedBrowserBuildDescriptor,
    activation: &BrowserReleaseActivationRow,
    manifest: &BrowserReleaseManifestRow,
) -> Result<bool> {
    let release_sha256 = hex::encode(Sha256::digest(manifest.release_id.as_bytes()));
    let count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_browser_release_revocations r
              WHERE (r.subject_kind = 'build-descriptor' AND r.subject_id = $1
                     AND r.subject_sha256 = $1)
                 OR (r.subject_kind = 'manifest' AND r.subject_id = $2
                     AND r.subject_sha256 = $3)
                 OR (r.subject_kind = 'release' AND r.subject_id = $4
                     AND r.subject_sha256 = $5)
                 OR (r.subject_kind = 'artifact' AND EXISTS (
                       SELECT 1 FROM jobs_browser_release_artifacts sibling
                        WHERE sibling.manifest_sha256 = $3
                          AND sibling.build_descriptor_sha256 = $1
                          AND sibling.artifact_id = r.subject_id
                          AND sibling.artifact_sha256 = r.subject_sha256
                     ))
                 OR (r.subject_kind = 'signing-key' AND (
                       r.subject_id = $6 OR EXISTS (
                         SELECT 1 FROM jobs_browser_release_signatures s
                          WHERE s.key_id = r.subject_id
                            AND s.signature_set_sha256 IN (
                              $7, $8,
                              (SELECT authorization_signature_set_sha256
                                 FROM jobs_browser_release_trust_policies
                                WHERE policy_sha256 = $9)
                            )
                       )
                     ))",
            &[
                &descriptor.descriptor_sha256,
                &manifest.manifest_id,
                &activation.manifest_sha256,
                &manifest.release_id,
                &release_sha256,
                &descriptor.signing_key_id,
                &activation.manifest_signature_set_sha256,
                &activation.authorization_signature_set_sha256,
                &activation.trust_policy_sha256,
            ],
        )?
        .get(0);
    if count > 0 {
        return Ok(true);
    }
    let revoked_key_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_browser_release_trust_keys k
              WHERE k.policy_sha256 = $1 AND k.state = 'revoked'
                AND (
                  k.key_id = $2 OR EXISTS (
                    SELECT 1 FROM jobs_browser_release_signatures s
                     WHERE s.key_id = k.key_id
                       AND s.signature_set_sha256 IN (
                         $3, $4,
                         (SELECT authorization_signature_set_sha256
                            FROM jobs_browser_release_trust_policies
                           WHERE policy_sha256 = $1)
                       )
                  )
                )",
            &[
                &activation.trust_policy_sha256,
                &descriptor.signing_key_id,
                &activation.manifest_signature_set_sha256,
                &activation.authorization_signature_set_sha256,
            ],
        )?
        .get(0);
    Ok(revoked_key_count > 0)
}

fn browser_release_claim_binding(
    assignment_sha256: String,
    assignment_generation: i64,
    channel: String,
    activation: BrowserReleaseActivationRow,
    manifest: BrowserReleaseManifestRow,
    artifact: BrowserReleaseArtifactRow,
    descriptor: &VerifiedBrowserBuildDescriptor,
) -> BrowserReleaseClaimBinding {
    BrowserReleaseClaimBinding {
        assignment_sha256,
        assignment_generation,
        channel,
        channel_head_revision: activation.channel_head_revision,
        channel_transition_sha256: activation.channel_transition_sha256,
        activation_sha256: activation.activation_sha256,
        activation_generation: activation.activation_generation,
        trust_generation: activation.trust_generation,
        channel_sequence: activation.channel_sequence,
        trust_policy_sha256: activation.trust_policy_sha256,
        manifest_signature_set_sha256: activation.manifest_signature_set_sha256,
        activation_authorization_signature_set_sha256: activation
            .authorization_signature_set_sha256,
        manifest_sha256: activation.manifest_sha256,
        release_sequence: manifest.release_sequence,
        artifact_id: artifact.artifact_id,
        artifact_sha256: artifact.artifact_sha256,
        artifact_url: artifact.artifact_url,
        artifact_filename: artifact.artifact_filename,
        artifact_size_bytes: artifact.artifact_size_bytes,
        package_kind: artifact.package_kind,
        release_id: descriptor.release_id.clone(),
        build_id: descriptor.build_id.clone(),
        app_version: descriptor.app_version.clone(),
        protocol_version: descriptor.protocol_version,
        platform: descriptor.platform.clone(),
        architecture: descriptor.architecture.clone(),
        build_descriptor_sha256: descriptor.descriptor_sha256.clone(),
        automation_bundle_sha256: artifact.automation_bundle_sha256,
        chromium_executable_sha256: artifact.chromium_executable_sha256,
        published_at_ms: manifest.published_at_ms,
    }
}

pub fn browser_release_binding_sha256(binding: &BrowserReleaseClaimBinding) -> String {
    let fields = [
        binding.assignment_sha256.clone(),
        binding.assignment_generation.to_string(),
        binding.channel.clone(),
        binding.channel_head_revision.to_string(),
        binding.channel_transition_sha256.clone(),
        binding.activation_sha256.clone(),
        binding.activation_generation.to_string(),
        binding.trust_generation.to_string(),
        binding.trust_policy_sha256.clone(),
        binding.channel_sequence.to_string(),
        binding.manifest_signature_set_sha256.clone(),
        binding
            .activation_authorization_signature_set_sha256
            .clone(),
        binding.manifest_sha256.clone(),
        binding.artifact_id.clone(),
        binding.artifact_sha256.clone(),
        binding.release_id.clone(),
        binding.build_id.clone(),
        binding.app_version.clone(),
        binding.protocol_version.to_string(),
        binding.platform.clone(),
        binding.architecture.clone(),
        binding.package_kind.clone(),
        binding.build_descriptor_sha256.clone(),
        binding.automation_bundle_sha256.clone(),
        binding.chromium_executable_sha256.clone(),
    ];
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-local-run-release-binding-v3\0");
    for field in fields {
        digest.update(field.as_bytes());
        digest.update(b"\0");
    }
    hex::encode(digest.finalize())
}

pub fn browser_release_receipt_authority(binding: &BrowserReleaseClaimBinding) -> Value {
    json!({
        "schemaVersion": 2,
        "bindingSha256": browser_release_binding_sha256(binding),
        "assignmentSha256": binding.assignment_sha256,
        "assignmentGeneration": binding.assignment_generation,
        "channel": binding.channel,
        "channelHeadRevision": binding.channel_head_revision,
        "channelTransitionSha256": binding.channel_transition_sha256,
        "activationSha256": binding.activation_sha256,
        "activationGeneration": binding.activation_generation,
        "trustGeneration": binding.trust_generation,
        "trustPolicySha256": binding.trust_policy_sha256,
        "channelSequence": binding.channel_sequence,
        "manifestSignatureSetSha256": binding.manifest_signature_set_sha256,
        "activationAuthorizationSignatureSetSha256":
            binding.activation_authorization_signature_set_sha256,
        "manifestSha256": binding.manifest_sha256,
        "releaseSequence": binding.release_sequence,
        "artifactId": binding.artifact_id,
        "artifactSha256": binding.artifact_sha256,
        "releaseId": binding.release_id,
        "buildId": binding.build_id,
        "appVersion": binding.app_version,
        "protocolVersion": binding.protocol_version,
        "platform": binding.platform,
        "architecture": binding.architecture,
        "packageKind": binding.package_kind,
        "descriptorSha256": binding.build_descriptor_sha256,
    })
}

pub fn browser_release_receipt_authority_valid(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let digest = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.len() == 64
                    && value == value.to_ascii_lowercase()
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    };
    let safe_text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| {
                !value.is_empty()
                    && value.len() <= 200
                    && value.trim() == value
                    && !value.chars().any(char::is_control)
            })
    };
    let positive = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0)
    };
    let platform = object.get("platform").and_then(Value::as_str);
    let architecture = object.get("architecture").and_then(Value::as_str);
    let package_kind = object.get("packageKind").and_then(Value::as_str);
    object.len() == 26
        && object.get("schemaVersion").and_then(Value::as_i64) == Some(2)
        && [
            "bindingSha256",
            "assignmentSha256",
            "channelTransitionSha256",
            "activationSha256",
            "trustPolicySha256",
            "manifestSignatureSetSha256",
            "activationAuthorizationSignatureSetSha256",
            "manifestSha256",
            "artifactSha256",
            "descriptorSha256",
        ]
        .iter()
        .all(|key| digest(key))
        && ["artifactId", "releaseId", "buildId", "appVersion"]
            .iter()
            .all(|key| safe_text(key))
        && [
            "assignmentGeneration",
            "channelHeadRevision",
            "activationGeneration",
            "trustGeneration",
            "channelSequence",
            "releaseSequence",
            "protocolVersion",
        ]
        .iter()
        .all(|key| positive(key))
        && matches!(
            object.get("channel").and_then(Value::as_str),
            Some("internal" | "beta" | "stable")
        )
        && matches!(platform, Some("darwin" | "windows"))
        && matches!(architecture, Some("arm64" | "x64"))
        && !(platform == Some("windows") && architecture != Some("x64"))
        && matches!(
            (platform, package_kind),
            (Some("darwin"), Some("darwin-dmg" | "darwin-zip"))
                | (Some("windows"), Some("windows-nsis"))
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedBrowserBuildDescriptor {
    release_id: String,
    build_id: String,
    app_version: String,
    app_id: String,
    protocol_version: i64,
    source_commit: String,
    platform: String,
    architecture: String,
    electron_version: String,
    playwright_version: String,
    chromium_revision: String,
    issued_at_ms: i64,
    signing_key_id: String,
}

fn parse_browser_build_descriptor_bytes(
    bytes: &[u8],
) -> Result<ParsedBrowserBuildDescriptor, BrowserReleaseAuthorityError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    let lines: Vec<&str> = text.split('\n').collect();
    const PREFIXES: [&str; 15] = [
        "version=",
        "audience=",
        "release_id=",
        "build_id=",
        "app_version=",
        "app_id=",
        "protocol_version=",
        "source_commit=",
        "platform=",
        "architecture=",
        "electron_version=",
        "playwright_version=",
        "chromium_revision=",
        "issued_at_ms=",
        "signing_key_id=",
    ];
    if lines.len() != PREFIXES.len() + 1 || !lines[PREFIXES.len()].is_empty() {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    let values = PREFIXES
        .iter()
        .enumerate()
        .map(|(index, prefix)| {
            lines[index]
                .strip_prefix(prefix)
                .filter(|value| !value.is_empty())
                .ok_or(BrowserReleaseAuthorityError::InvalidBuildProof)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values[0] != "1" || values[1] != BROWSER_BUILD_AUDIENCE {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    let release_id = values[2];
    let build_id = values[3];
    let app_version = values[4];
    let app_id = values[5];
    let protocol_version = browser_release_positive_integer(values[6])?;
    let source_commit = values[7];
    let platform = values[8];
    let architecture = values[9];
    let electron_version = values[10];
    let playwright_version = values[11];
    let chromium_revision = values[12];
    let issued_at_ms = browser_release_nonnegative_integer(values[13])?;
    let signing_key_id = values[14];
    if !browser_build_descriptor_release_id(release_id)
        || !browser_release_build_id(build_id)
        || !browser_release_semver(app_version)
        || app_id != BROWSER_APP_ID
        || !browser_release_source_commit(source_commit)
        || !matches!(platform, "darwin" | "windows")
        || !matches!(architecture, "arm64" | "x64")
        || (platform == "windows" && architecture != "x64")
        || !browser_release_semver(electron_version)
        || !browser_release_semver(playwright_version)
        || chromium_revision.len() > 13
        || browser_release_nonnegative_integer(chromium_revision).is_err()
        || !browser_release_safe_id(signing_key_id)
    {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    Ok(ParsedBrowserBuildDescriptor {
        release_id: release_id.to_string(),
        build_id: build_id.to_string(),
        app_version: app_version.to_string(),
        app_id: app_id.to_string(),
        protocol_version,
        source_commit: source_commit.to_string(),
        platform: platform.to_string(),
        architecture: architecture.to_string(),
        electron_version: electron_version.to_string(),
        playwright_version: playwright_version.to_string(),
        chromium_revision: chromium_revision.to_string(),
        issued_at_ms,
        signing_key_id: signing_key_id.to_string(),
    })
}

fn browser_build_descriptor_release_id(value: &str) -> bool {
    browser_release_safe_id(value)
        && ![
            "beta", "current", "download", "internal", "latest", "stable",
        ]
        .iter()
        .any(|reserved| value.eq_ignore_ascii_case(reserved))
}

fn browser_release_decode_base64url_exact(
    value: &str,
    expected_bytes: usize,
) -> Result<Vec<u8>, BrowserReleaseAuthorityError> {
    let decoded = browser_release_decode_base64url_bounded(value, expected_bytes)?;
    if decoded.len() != expected_bytes {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    Ok(decoded)
}

fn browser_release_decode_base64url_bounded(
    value: &str,
    maximum_bytes: usize,
) -> Result<Vec<u8>, BrowserReleaseAuthorityError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    if decoded.is_empty()
        || decoded.len() > maximum_bytes
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    Ok(decoded)
}

fn browser_release_safe_id(value: &str) -> bool {
    (3..=128).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn browser_release_source_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn browser_release_build_id(value: &str) -> bool {
    let Some(components) = value.strip_prefix("browser-") else {
        return false;
    };
    let mut parts = components.split('.');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    parts.next().is_none()
        && first.len() <= 9
        && second.len() <= 9
        && browser_release_nonnegative_integer(first).is_ok()
        && browser_release_nonnegative_integer(second).is_ok()
}

fn browser_release_semver(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| part.len() <= 9 && browser_release_nonnegative_integer(part).is_ok())
}

fn browser_release_nonnegative_integer(value: &str) -> Result<i64, BrowserReleaseAuthorityError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)?;
    if parsed > BROWSER_MAX_SAFE_INTEGER {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    i64::try_from(parsed).map_err(|_| BrowserReleaseAuthorityError::InvalidBuildProof)
}

fn browser_release_positive_integer(value: &str) -> Result<i64, BrowserReleaseAuthorityError> {
    let parsed = browser_release_nonnegative_integer(value)?;
    if parsed < 1 {
        return Err(BrowserReleaseAuthorityError::InvalidBuildProof);
    }
    Ok(parsed)
}

#[cfg(test)]
mod browser_release_authority_tests {
    use super::*;

    #[test]
    fn postgres_availability_samples_database_time_after_fleet_and_registry_fences() {
        let source = include_str!("browser_release_authority.rs");
        let section = source
            .split("fn local_browser_release_availability_inner(")
            .nth(1)
            .expect("Browser release availability")
            .split("fn sqlite_local_browser_release_availability(")
            .next()
            .expect("bounded Browser release availability")
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL Browser release availability");
        let mut previous = 0;
        for operation in [
            "postgres_runner_volume_fleet_distribution_ready",
            "postgres_lock_browser_release_registry_shared",
            "local_run_claim_db_now_postgres",
            "postgres_local_browser_release_availability",
        ] {
            let position = section
                .find(operation)
                .unwrap_or_else(|| panic!("missing Browser availability operation {operation}"));
            assert!(
                position >= previous,
                "Browser availability time sampled before {operation}"
            );
            previous = position;
        }
        assert!(!section.contains("now_ms()"));
    }

    #[test]
    fn postgres_claim_locks_effect_rows_before_final_database_time_and_never_relocks() {
        let source = include_str!("browser_release_authority.rs");
        let outer = source
            .split("fn claim_local_run_with_browser_release_inner")
            .nth(1)
            .expect("Browser claim transaction")
            .split("fn browser_claim_request_sha256")
            .next()
            .expect("bounded Browser claim transaction")
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL Browser claim transaction");
        let mut previous = 0;
        for operation in [
            "lock_operational_hold_shared_postgres_tx",
            "lock_managed_cloud_release_registry_shared_postgres_tx",
            "lock_postgres_ats_certification",
            "SELECT account_id FROM jobs_local_run_tickets",
            "lock_discovery_account_shared_postgres",
            "postgres_claim_local_run_with_browser_release",
        ] {
            let position = outer
                .find(operation)
                .unwrap_or_else(|| panic!("missing Browser claim operation {operation}"));
            assert!(position >= previous, "Browser claim prelock order inverted");
            previous = position;
        }

        let claim = source
            .split("fn postgres_claim_local_run_with_browser_release")
            .nth(1)
            .expect("PostgreSQL Browser claim")
            .split("fn postgres_lock_browser_release_registry_shared")
            .next()
            .expect("bounded PostgreSQL Browser claim");
        let fresh_claim = claim
            .split("if require_distribution_ready")
            .nth(1)
            .expect("fresh PostgreSQL Browser claim");
        let mut previous = 0;
        for operation in [
            "SELECT id FROM accounts",
            "postgres_local_run_authority_prelock",
            "postgres_lock_browser_release_registry_shared",
            "local_run_claim_db_now_postgres",
            "postgres_local_run_authority_after_prelock_at_ms",
            "postgres_browser_runner_claim_operational_block",
            "INSERT INTO jobs_local_run_release_bindings",
        ] {
            let position = fresh_claim
                .find(operation)
                .unwrap_or_else(|| panic!("missing Browser claim operation {operation}"));
            assert!(
                position >= previous,
                "Browser claim row/time order inverted"
            );
            previous = position;
        }
        assert!(claim.contains(
            "create_ats_application_certification_binding_from_context_postgres_tx_after_prelock"
        ));
        for forbidden in [
            "lock_operational_hold_shared_postgres_tx",
            "lock_managed_cloud_release_registry_shared_postgres_tx",
            "lock_postgres_ats_certification",
            "lock_discovery_account_shared_postgres",
            "operational_hold_context_for_application_postgres_tx(",
            "create_ats_application_certification_binding_from_context_postgres_tx(",
        ] {
            assert!(
                !claim.contains(forbidden),
                "Browser claim relocks {forbidden}"
            );
        }

        let sqlite_replay_domain = source
            .split("fn sqlite_browser_replay_employer_domain(")
            .nth(1)
            .expect("SQLite Browser replay domain")
            .split("fn postgres_browser_replay_employer_domain(")
            .next()
            .expect("bounded SQLite Browser replay domain");
        assert!(sqlite_replay_domain.contains("application.state == \"submitted\""));
        assert!(sqlite_replay_domain.contains("submitted_execution_employer_domain"));
        assert!(sqlite_replay_domain
            .contains("resolve_current_execution_authority_sqlite_after_prelock"));

        let postgres_replay_domain = source
            .split("fn postgres_browser_replay_employer_domain(")
            .nth(1)
            .expect("PostgreSQL Browser replay domain")
            .split("fn sqlite_claim_local_run_with_browser_release")
            .next()
            .expect("bounded PostgreSQL Browser replay domain");
        assert!(postgres_replay_domain.contains("application.state == \"submitted\""));
        assert!(postgres_replay_domain.contains("submitted_execution_employer_domain"));
        assert!(postgres_replay_domain
            .contains("resolve_current_execution_authority_postgres_after_prelock"));
        for forbidden in [
            "lock_operational_hold_shared_postgres_tx",
            "lock_managed_cloud_release_registry_shared_postgres_tx",
            "lock_postgres_ats_certification",
            "lock_discovery_account_shared_postgres",
        ] {
            assert!(
                !postgres_replay_domain.contains(forbidden),
                "Browser replay domain relocks {forbidden}"
            );
        }
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BrowserReleaseVector {
        descriptor: String,
        signature: String,
        public_key: String,
        descriptor_sha256: String,
    }

    fn vector() -> BrowserReleaseVector {
        serde_json::from_str(include_str!(
            "../../../../jobs/browser/fixtures/release-authority-v1.json"
        ))
        .expect("shared Browser release vector")
    }

    fn key_ring(vector: &BrowserReleaseVector) -> BrowserBuildVerifyingKeyRing {
        BrowserBuildVerifyingKeyRing::new(BTreeMap::from([(
            "release-key-1".to_string(),
            vector.public_key.clone(),
        )]))
        .expect("valid test key ring")
    }

    #[test]
    fn node_and_rust_verify_the_same_exact_build_proof() {
        let vector = vector();
        let proof = BrowserBuildProof {
            descriptor: vector.descriptor.clone(),
            signature: vector.signature.clone(),
        };
        let verified = verify_browser_build_proof(&proof, &key_ring(&vector)).unwrap();
        assert_eq!(verified.release_id, "browser-release-603-1");
        assert_eq!(verified.build_id, "browser-603.1");
        assert_eq!(verified.app_version, "0.1.0");
        assert_eq!(verified.app_id, BROWSER_APP_ID);
        assert_eq!(verified.protocol_version, 1);
        assert_eq!(verified.platform, "darwin");
        assert_eq!(verified.architecture, "arm64");
        assert_eq!(verified.descriptor_sha256, vector.descriptor_sha256);
    }

    #[test]
    fn build_proof_rejects_unknown_key_tampering_and_noncanonical_bytes() {
        let vector = vector();
        let proof = BrowserBuildProof {
            descriptor: vector.descriptor.clone(),
            signature: vector.signature.clone(),
        };
        let unknown = BrowserBuildVerifyingKeyRing::new(BTreeMap::from([(
            "another-browser-build-key".to_string(),
            vector.public_key.clone(),
        )]))
        .unwrap();
        assert_eq!(
            verify_browser_build_proof(&proof, &unknown),
            Err(BrowserReleaseAuthorityError::UnknownBuildSigningKey)
        );

        let mut tampered_signature = proof.clone();
        tampered_signature.signature.replace_range(0..1, "A");
        assert_eq!(
            verify_browser_build_proof(&tampered_signature, &key_ring(&vector)),
            Err(BrowserReleaseAuthorityError::InvalidBuildSignature)
        );

        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&proof.descriptor)
            .unwrap();
        let noncanonical =
            String::from_utf8(decoded)
                .unwrap()
                .replacen("version=1", "version=01", 1);
        let mut noncanonical_proof = proof;
        noncanonical_proof.descriptor =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(noncanonical);
        assert_eq!(
            verify_browser_build_proof(&noncanonical_proof, &key_ring(&vector)),
            Err(BrowserReleaseAuthorityError::InvalidBuildProof)
        );
    }
}
