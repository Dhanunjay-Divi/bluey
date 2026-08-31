type ExecutionLeaseResult<T> = std::result::Result<T, ExecutionLeaseError>;

fn execution_lease_from_operational_hold_error(error: OperationalHoldError) -> ExecutionLeaseError {
    match error {
        OperationalHoldError::Storage(error) => ExecutionLeaseError::Storage(error),
        OperationalHoldError::InvalidRequest
        | OperationalHoldError::NotFound
        | OperationalHoldError::Conflict
        | OperationalHoldError::IdentityConflict
        | OperationalHoldError::Held(_) => ExecutionLeaseError::Conflict,
    }
}

fn execution_lease_from_managed_cloud_error(
    error: ManagedCloudRegistryError,
) -> ExecutionLeaseError {
    match error {
        ManagedCloudRegistryError::InvalidRequest => ExecutionLeaseError::InvalidRequest,
        ManagedCloudRegistryError::NotFound => ExecutionLeaseError::NotFound,
        ManagedCloudRegistryError::Storage(error) => ExecutionLeaseError::Storage(error),
        ManagedCloudRegistryError::InvalidEnvelope
        | ManagedCloudRegistryError::InvalidAuthority
        | ManagedCloudRegistryError::IdentityConflict
        | ManagedCloudRegistryError::CompareAndSwapConflict
        | ManagedCloudRegistryError::SequenceRegression
        | ManagedCloudRegistryError::DowngradeRequiresRollback
        | ManagedCloudRegistryError::Revoked
        | ManagedCloudRegistryError::Unavailable
        | ManagedCloudRegistryError::CohortIneligible
        | ManagedCloudRegistryError::GrantExpired
        | ManagedCloudRegistryError::GrantConsumed
        | ManagedCloudRegistryError::HeartbeatSequenceConflict
        | ManagedCloudRegistryError::RecoveryNotAccepted => ExecutionLeaseError::Conflict,
    }
}

fn execution_lease_from_original_source_error(
    error: OriginalSourceVerificationError,
) -> ExecutionLeaseError {
    ExecutionLeaseError::Storage(anyhow::Error::new(error))
}

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IrreversibleExecutionLeaseRecord {
    #[serde(flatten)]
    pub lease: ExecutionLeaseRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ats_certified_receipt_authority: Option<AtsCertifiedReceiptAuthority>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthorizedRunnerVolumeExecutionLeaseGrant {
    #[serde(flatten)]
    pub grant: RunnerVolumeExecutionLeaseGrant,
    #[serde(flatten)]
    pub managed_cloud: Option<ManagedCloudExecutionLeaseAuthority>,
}

impl std::ops::Deref for AuthorizedRunnerVolumeExecutionLeaseGrant {
    type Target = RunnerVolumeExecutionLeaseGrant;

    fn deref(&self) -> &Self::Target {
        &self.grant
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthorizedExecutionLeaseRecord {
    #[serde(flatten)]
    pub lease: ExecutionLeaseRecord,
    #[serde(flatten)]
    pub managed_cloud: Option<ManagedCloudExecutionLeaseAuthority>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthorizedIrreversibleExecutionLeaseRecord {
    #[serde(flatten)]
    pub record: IrreversibleExecutionLeaseRecord,
    #[serde(flatten)]
    pub managed_cloud: Option<ManagedCloudExecutionLeaseAuthority>,
}

#[derive(Clone, Copy)]
struct ManagedExecutionLeaseClaimContext<'a> {
    input: Option<&'a ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &'a str,
    volume_worker_id: &'a str,
}

type ExecutionLeaseClaimOutcome = (
    ExecutionLeaseGrant,
    Option<RunnerVolumeResidencyBinding>,
    Option<TrustedRunnerProcessRuntimeAttestation>,
    Option<ManagedCloudExecutionLeaseAuthority>,
);

impl std::ops::Deref for IrreversibleExecutionLeaseRecord {
    type Target = ExecutionLeaseRecord;

    fn deref(&self) -> &Self::Target {
        &self.lease
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinalSubmitSurfaceCanonical<'a> {
    adapter: &'a str,
    adapter_version: &'a str,
    control: &'a str,
    fields: Vec<FinalSubmitSurfaceField<'a>>,
    files: Vec<FinalSubmitSurfaceField<'a>>,
    form: FinalSubmitSurfaceForm<'a>,
    part_order: Vec<FinalSubmitSurfacePart<'a>>,
    schema_version: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinalSubmitSurfaceField<'a> {
    field_name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinalSubmitSurfaceForm<'a> {
    enctype: &'a str,
    form_identity_sha256: String,
    form_target: &'a str,
    method: &'a str,
}

#[derive(Serialize)]
struct FinalSubmitSurfacePart<'a> {
    index: i64,
    kind: &'a str,
}

fn final_submit_surface_sha256(proof: &FinalSubmitProof) -> ExecutionLeaseResult<String> {
    let canonical = FinalSubmitSurfaceCanonical {
        adapter: &proof.adapter,
        adapter_version: &proof.adapter_version,
        control: &proof.control,
        fields: proof
            .fields
            .iter()
            .map(|field| FinalSubmitSurfaceField {
                field_name: &field.field_name,
            })
            .collect(),
        files: proof
            .files
            .iter()
            .map(|file| FinalSubmitSurfaceField {
                field_name: &file.field_name,
            })
            .collect(),
        form: FinalSubmitSurfaceForm {
            enctype: &proof.target.enctype,
            form_identity_sha256: hex::encode(Sha256::digest(
                proof.target.form_identity.as_bytes(),
            )),
            form_target: &proof.target.form_target,
            method: &proof.target.method,
        },
        part_order: proof
            .part_order
            .iter()
            .map(|part| FinalSubmitSurfacePart {
                index: part.index,
                kind: &part.kind,
            })
            .collect(),
        schema_version: 1,
    };
    let bytes = serde_json::to_vec(&canonical).map_err(|_| ExecutionLeaseError::InvalidRequest)?;
    Ok(hex::encode(Sha256::digest(bytes)))
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
    let certified = proof.schema_version == 4;
    let frozen_certified = application_has_frozen_ats_certification(application);
    if !((proof.schema_version == 3
        && proof.certification.is_none()
        && proof.observed_surface.is_none())
        || (certified && proof.certification.is_some() && proof.observed_surface.is_some()))
        || frozen_certified != certified
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
    if certified {
        validate_certified_final_submit_binding(application, proof, expected_provider.0)?;
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

pub fn application_has_frozen_ats_certification(application: &JobApplication) -> bool {
    application
        .receipt
        .pointer("/approved_execution/schema_version")
        .and_then(Value::as_i64)
        == Some(3)
        && application
            .receipt
            .pointer("/approved_execution/admission/kind")
            .and_then(Value::as_str)
            == Some("track_auto_submit")
        && application
            .receipt
            .pointer("/approved_execution/admission/ats_certification")
            .is_some_and(Value::is_object)
}

fn validate_certified_final_submit_binding(
    application: &JobApplication,
    proof: &FinalSubmitProof,
    expected_provider: &str,
) -> ExecutionLeaseResult<()> {
    let certification = proof
        .certification
        .as_ref()
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    let observed_surface = proof
        .observed_surface
        .as_ref()
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    if certification.schema_version != 1
        || certification.provider != expected_provider
        || certification.adapter_version != proof.adapter_version
        || certification.activation_generation <= 0
        || certification.expires_at_ms <= now_ms()
        || observed_surface.schema_version != 1
        || !validate_execution_binding(&observed_surface.variant_key, 120)
        || observed_surface.layout_contract_version <= 0
        || !valid_final_submit_sha256(&observed_surface.surface_sha256)
        || final_submit_surface_sha256(proof)? != observed_surface.surface_sha256
        || certification.runner_target_sha256s.is_empty()
        || certification.runner_target_sha256s.len() > 2
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    for digest in [
        certification.manifest_sha256.as_str(),
        certification.activation_sha256.as_str(),
        certification.target_key_sha256.as_str(),
        certification.layout_set_sha256.as_str(),
        certification.adapter_bundle_sha256.as_str(),
    ] {
        if !valid_final_submit_sha256(digest) {
            return Err(ExecutionLeaseError::InvalidRequest);
        }
    }
    let mut previous: Option<&str> = None;
    for digest in &certification.runner_target_sha256s {
        if !valid_final_submit_sha256(digest)
            || previous.is_some_and(|previous| previous >= digest.as_str())
        {
            return Err(ExecutionLeaseError::InvalidRequest);
        }
        previous = Some(digest);
    }

    let admission = application
        .receipt
        .pointer("/approved_execution/admission/ats_certification")
        .and_then(Value::as_object)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let exact = admission.get("schema_version").and_then(Value::as_i64)
        == Some(certification.schema_version)
        && admission.get("provider").and_then(Value::as_str)
            == Some(certification.provider.as_str())
        && admission.get("adapter_version").and_then(Value::as_str)
            == Some(certification.adapter_version.as_str())
        && admission.get("manifest_sha256").and_then(Value::as_str)
            == Some(certification.manifest_sha256.as_str())
        && admission.get("activation_sha256").and_then(Value::as_str)
            == Some(certification.activation_sha256.as_str())
        && admission
            .get("activation_generation")
            .and_then(Value::as_i64)
            == Some(certification.activation_generation)
        && admission.get("target_key_sha256").and_then(Value::as_str)
            == Some(certification.target_key_sha256.as_str())
        && admission.get("layout_set_sha256").and_then(Value::as_str)
            == Some(certification.layout_set_sha256.as_str())
        && admission.get("variant_key").and_then(Value::as_str)
            == Some(observed_surface.variant_key.as_str())
        && admission
            .get("layout_contract_version")
            .and_then(Value::as_i64)
            == Some(observed_surface.layout_contract_version)
        && admission.get("surface_sha256").and_then(Value::as_str)
            == Some(observed_surface.surface_sha256.as_str())
        && admission
            .get("adapter_bundle_sha256")
            .and_then(Value::as_str)
            == Some(certification.adapter_bundle_sha256.as_str())
        && admission
            .get("runner_target_sha256s")
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values.len() == certification.runner_target_sha256s.len()
                    && values
                        .iter()
                        .zip(&certification.runner_target_sha256s)
                        .all(|(actual, expected)| actual.as_str() == Some(expected.as_str()))
            })
        && admission.get("expires_at_ms").and_then(Value::as_i64)
            == Some(certification.expires_at_ms);
    if !exact {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn final_submit_file_kind_and_hash(name: &str) -> Option<(&'static str, &str)> {
    let stem = name.strip_suffix(".pdf")?;
    let (kind, sha256) = if let Some(sha256) = stem.strip_prefix("resume-") {
        ("resume", sha256)
    } else {
        let sha256 = stem.strip_prefix("cover-letter-")?;
        ("cover_letter", sha256)
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

pub(crate) fn final_submit_provider_job_key(
    provider: &str,
    raw_url: &str,
) -> std::result::Result<String, ExecutionLeaseError> {
    final_submit_provider_job_key_for_purpose(
        provider,
        raw_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
}

#[cfg(test)]
pub(crate) fn final_submit_confirmation_provider_job_key(
    provider: &str,
    raw_url: &str,
) -> std::result::Result<String, ExecutionLeaseError> {
    final_submit_provider_job_key_for_purpose(
        provider,
        raw_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Confirmation,
    )
}

fn final_submit_provider_job_key_for_purpose(
    provider: &str,
    raw_url: &str,
    purpose: crate::jobs_ats_target::ProviderApplicationTargetPurpose,
) -> ExecutionLeaseResult<String> {
    let target = crate::jobs_ats_target::parse_provider_application_target(raw_url, purpose)
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    (target.provider == provider)
        .then_some(target.provider_job_key)
        .ok_or(ExecutionLeaseError::InvalidRequest)
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

struct ExecutionTargetAuthority {
    application: JobApplication,
    browser_profile_id: String,
    employer_domain: OperationalHoldEmployerDomain,
}

fn job_application_snapshot_matches(discovered: &JobApplication, locked: &JobApplication) -> bool {
    discovered.id == locked.id
        && discovered.job_id == locked.job_id
        && discovered.resume_version_id == locked.resume_version_id
        && discovered.state == locked.state
        && discovered.submission_mode == locked.submission_mode
        && discovered.match_score == locked.match_score
        && discovered.answers == locked.answers
        && discovered.cover_letter == locked.cover_letter
        && discovered.receipt == locked.receipt
        && discovered.run_id == locked.run_id
        && discovered.created_at_ms == locked.created_at_ms
        && discovered.updated_at_ms == locked.updated_at_ms
        && discovered.submitted_at_ms == locked.submitted_at_ms
}

fn discover_postgres_execution_application(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> ExecutionLeaseResult<JobApplication> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let job_id: String = row.get(0);
    Ok(parse_application_json(
        row.get(1),
        application_id,
        &job_id,
        "discovered execution application",
    )?)
}

fn sqlite_execution_target(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<ExecutionTargetAuthority> {
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
        .ok_or(ExecutionLeaseError::Conflict)?
        .to_string();
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
            params![account_id, &identity_id],
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
    let current = resolve_current_execution_authority_sqlite_after_prelock(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )?;
    if !current.authorized {
        return Err(ExecutionLeaseError::Conflict);
    }
    let employer_domain = current
        .employer_domain
        .ok_or(ExecutionLeaseError::Conflict)?;
    Ok(ExecutionTargetAuthority {
        application,
        browser_profile_id: execution_browser_profile_id(account_id, &identity_id),
        employer_domain,
    })
}

fn postgres_execution_target(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<ExecutionTargetAuthority> {
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
    let current = resolve_current_execution_authority_postgres_after_prelock(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )?;
    if !current.authorized {
        return Err(ExecutionLeaseError::Conflict);
    }
    let employer_domain = current
        .employer_domain
        .ok_or(ExecutionLeaseError::Conflict)?;
    Ok(ExecutionTargetAuthority {
        application,
        browser_profile_id: execution_browser_profile_id(account_id, &identity_id),
        employer_domain,
    })
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

fn prelock_postgres_cloud_attempt(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> ExecutionLeaseResult<()> {
    let row = tx
        .query_opt(
            "SELECT runner, status FROM jobs_attempt_reservations
              WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?;
    let runner: String = row.get(0);
    let status: String = row.get(1);
    if !matches!(status.as_str(), "reserved" | "running")
        || !matches!(runner.as_str(), "unassigned" | "cloud")
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn prelock_postgres_cloud_ats_phase_a_rows(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    run_id: &str,
) -> ExecutionLeaseResult<()> {
    tx.query_opt(
        "SELECT id FROM jobs_postings WHERE account_id = $1 AND id = $2 FOR UPDATE",
        &[&account_id, &application.job_id],
    )?
    .ok_or(ExecutionLeaseError::NotFound)?;
    tx.query(
        "SELECT binding_id FROM jobs_application_ats_certification_bindings
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
          ORDER BY binding_id FOR UPDATE",
        &[&account_id, &application.id, &run_id],
    )?;
    Ok(())
}

fn prelock_postgres_execution_lease_set(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<()> {
    tx.query(
        "SELECT run_id FROM jobs_execution_leases
          WHERE account_id = $1 AND (application_id = $2 OR browser_profile_id = $3)
          ORDER BY run_id FOR UPDATE",
        &[&account_id, &application_id, &browser_profile_id],
    )?;
    Ok(())
}

fn sqlite_application_requires_ats_phase_a(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> ExecutionLeaseResult<bool> {
    let (job_id, application_json): (String, String) = tx
        .query_row(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        application_json,
        application_id,
        &job_id,
        "ATS Phase A cloud application",
    )?;
    Ok(application.submission_mode == "auto_submit"
        && application_has_frozen_ats_certification(&application))
}

fn postgres_application_requires_ats_phase_a(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> ExecutionLeaseResult<bool> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        application_id,
        &job_id,
        "ATS Phase A cloud application",
    )?;
    Ok(application.submission_mode == "auto_submit"
        && application_has_frozen_ats_certification(&application))
}

fn cloud_ats_runtime_attestation(
    trusted: &TrustedRunnerProcessRuntimeAttestation,
) -> AtsCertificationRuntimeAttestation {
    AtsCertificationRuntimeAttestation::Cloud {
        platform: trusted.runtime.platform.clone(),
        architecture: trusted.runtime.architecture.clone(),
        runner_build_id: trusted.runtime.runner_build_id.clone(),
        runner_image_sha256: trusted.runtime.runner_image_sha256.clone(),
        automation_bundle_sha256: trusted.runtime.automation_bundle_sha256.clone(),
        playwright_version: trusted.runtime.playwright_version.clone(),
        chromium_revision: trusted.runtime.chromium_revision.clone(),
        chromium_executable_sha256: trusted.runtime.chromium_executable_sha256.clone(),
    }
}

struct CloudAtsPhaseAContext<'a> {
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    browser_profile_id: &'a str,
    trusted_runtime: Option<&'a TrustedRunnerProcessRuntimeAttestation>,
    nonce_sha256: &'a str,
    now_ms: i64,
}

fn create_cloud_ats_phase_a_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    context: &CloudAtsPhaseAContext<'_>,
) -> ExecutionLeaseResult<()> {
    if !sqlite_application_requires_ats_phase_a(tx, context.account_id, context.application_id)? {
        return Ok(());
    }
    let trusted_runtime = context
        .trusted_runtime
        .ok_or(ExecutionLeaseError::Conflict)?;
    create_ats_application_certification_binding_from_context_sqlite_tx(
        tx,
        &AtsCertificationPhaseAContextRequest {
            account_id: context.account_id.to_string(),
            application_id: context.application_id.to_string(),
            run_id: context.run_id.to_string(),
            browser_session_id: context.run_id.to_string(),
            browser_profile_id: context.browser_profile_id.to_string(),
            runtime_attestation: cloud_ats_runtime_attestation(trusted_runtime),
            nonce_sha256: context.nonce_sha256.to_string(),
        },
        context.now_ms,
    )
    .map(|_| ())
    .map_err(execution_lease_from_ats_certification_error)
}

fn create_cloud_ats_phase_a_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    context: &CloudAtsPhaseAContext<'_>,
) -> ExecutionLeaseResult<()> {
    if !postgres_application_requires_ats_phase_a(tx, context.account_id, context.application_id)? {
        return Ok(());
    }
    let trusted_runtime = context
        .trusted_runtime
        .ok_or(ExecutionLeaseError::Conflict)?;
    create_ats_application_certification_binding_from_context_postgres_tx_after_prelock(
        tx,
        &AtsCertificationPhaseAContextRequest {
            account_id: context.account_id.to_string(),
            application_id: context.application_id.to_string(),
            run_id: context.run_id.to_string(),
            browser_session_id: context.run_id.to_string(),
            browser_profile_id: context.browser_profile_id.to_string(),
            runtime_attestation: cloud_ats_runtime_attestation(trusted_runtime),
            nonce_sha256: context.nonce_sha256.to_string(),
        },
        context.now_ms,
    )
    .map(|_| ())
    .map_err(execution_lease_from_ats_certification_error)
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
        None,
        None,
    )
    .map(|(lease, _, _, _)| lease)
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
    let (lease, binding, _, _) = claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        Some(volume_binding),
        None,
        None,
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
        runtime_grant_id: String::new(),
        runtime_sha256: String::new(),
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
    expected_runtime_grant_id: &str,
    expected_runtime_sha256: &str,
    volume_authority: &VerifiedRunnerVolumeAuthority,
) -> ExecutionLeaseResult<RunnerVolumeExecutionLeaseGrant> {
    let (lease, binding, runtime, _) = claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        Some(volume_binding),
        Some((expected_runtime_grant_id, expected_runtime_sha256)),
        Some(volume_authority),
        None,
    )?;
    let binding = binding.ok_or(ExecutionLeaseError::Conflict)?;
    let runtime = runtime.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(RunnerVolumeExecutionLeaseGrant {
        lease,
        purge_subject: binding.purge_subject,
        volume_id: binding.volume_id,
        enrollment_epoch: binding.enrollment_epoch,
        process_instance_id: binding.process_instance_id,
        volume_key_fingerprint: binding.volume_key_fingerprint,
        runtime_grant_id: runtime.runtime_grant_id,
        runtime_sha256: runtime.runtime_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn claim_managed_execution_lease_for_runner_volume_authorized(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
    volume_binding: &BindRunnerVolumeResidencyRequest,
    expected_runtime_grant_id: &str,
    expected_runtime_sha256: &str,
    volume_authority: &VerifiedRunnerVolumeAuthority,
    managed_cloud: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
    volume_worker_id: &str,
) -> ExecutionLeaseResult<AuthorizedRunnerVolumeExecutionLeaseGrant> {
    let (lease, binding, runtime, managed_cloud) = claim_execution_lease_inner(
        pool,
        account_id,
        application_id,
        run_id,
        supplied_browser_profile_id,
        owner_id,
        Some(volume_binding),
        Some((expected_runtime_grant_id, expected_runtime_sha256)),
        Some(volume_authority),
        Some(ManagedExecutionLeaseClaimContext {
            input: managed_cloud,
            authenticated_worker_id,
            volume_worker_id,
        }),
    )?;
    let binding = binding.ok_or(ExecutionLeaseError::Conflict)?;
    let runtime = runtime.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(AuthorizedRunnerVolumeExecutionLeaseGrant {
        grant: RunnerVolumeExecutionLeaseGrant {
            lease,
            purge_subject: binding.purge_subject,
            volume_id: binding.volume_id,
            enrollment_epoch: binding.enrollment_epoch,
            process_instance_id: binding.process_instance_id,
            volume_key_fingerprint: binding.volume_key_fingerprint,
            runtime_grant_id: runtime.runtime_grant_id,
            runtime_sha256: runtime.runtime_sha256,
        },
        managed_cloud,
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
    expected_runtime: Option<(&str, &str)>,
    volume_authority: Option<&VerifiedRunnerVolumeAuthority>,
    managed_cloud_context: Option<ManagedExecutionLeaseClaimContext<'_>>,
) -> ExecutionLeaseResult<ExecutionLeaseClaimOutcome> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || !validate_execution_binding(supplied_browser_profile_id, 160)
        || !validate_execution_binding(owner_id, 240)
        || volume_binding
            .is_some_and(|binding| binding.account_id != account_id || binding.run_id != run_id)
        || (volume_authority.is_some() && expected_runtime.is_none())
        || (expected_runtime.is_some() && volume_binding.is_none())
        || managed_cloud_context.is_some_and(|context| {
            volume_binding.is_none()
                || context.authenticated_worker_id != context.volume_worker_id
                || volume_binding
                    .is_some_and(|binding| binding.worker_id != context.volume_worker_id)
        })
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
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let execution_target =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            if execution_target.browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            let hold_context = operational_hold_context_for_application_sqlite_tx_after_authority(
                &tx,
                account_id,
                application_id,
                &execution_target.employer_domain,
                Some("cloud"),
                None,
                None,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            require_operational_capability_sqlite_tx(
                &tx,
                OperationalCapability::RunnerClaim,
                &hold_context,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            let browser_profile_id = execution_target.browser_profile_id;
            let prepared_binding = match (volume_binding, proposed_subject.as_deref()) {
                (Some(binding), Some(subject)) => Some(
                    prepare_runner_volume_lease_binding_sqlite_tx(&tx, binding, subject)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            let trusted_runtime = match (volume_binding, expected_runtime) {
                (Some(binding), Some((expected_grant_id, expected_runtime_sha256))) => {
                    let runtime = require_runner_process_runtime_sqlite_tx(
                        &tx,
                        &binding.worker_id,
                        &binding.volume_id,
                        binding.enrollment_epoch,
                        &binding.process_instance_id,
                    )
                    .map_err(execution_lease_from_runner_volume_error)?;
                    if runtime.runtime_grant_id != expected_grant_id
                        || runtime.runtime_sha256 != expected_runtime_sha256
                    {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    Some(runtime)
                }
                (Some(_), None) if cfg!(debug_assertions) && volume_authority.is_none() => None,
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
            sqlite_bind_cloud_attempt(&tx, account_id, application_id, now)?;
            create_cloud_ats_phase_a_sqlite_tx(
                &tx,
                &CloudAtsPhaseAContext {
                    account_id,
                    application_id,
                    run_id,
                    browser_profile_id: &browser_profile_id,
                    trusted_runtime: trusted_runtime.as_ref(),
                    nonce_sha256: &lease_token_sha256,
                    now_ms: now,
                },
            )?;
            let managed_cloud_authority = match managed_cloud_context {
                Some(context) => resolve_managed_cloud_execution_lease_claim_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    run_id,
                    context.input,
                    context.authenticated_worker_id,
                    context.volume_worker_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?,
                None => None,
            };
            let existing = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?;
            let (fence, bound_managed_cloud) = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                let bound_managed_cloud = managed_cloud_authority
                    .clone()
                    .map(|authority| {
                        bind_managed_cloud_execution_lease_authority(
                            authority,
                            run_id,
                            fence,
                            &lease_token_sha256,
                        )
                    })
                    .transpose()
                    .map_err(execution_lease_from_managed_cloud_error)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = ?2, lease_token_sha256 = ?3, fence = ?4,
                            lease_expires_at_ms = ?5, updated_at_ms = ?6,
                            managed_cloud_workflow_request_id = ?8,
                            managed_cloud_request_command_id = ?9,
                            managed_cloud_execution_command_id = ?10,
                            managed_cloud_binding_sha256 = ?11,
                            managed_cloud_release_memo_base64url = ?12,
                            managed_cloud_release_sha256 = ?13,
                            managed_cloud_runtime_instance_id = ?14,
                            managed_cloud_runtime_instance_epoch = ?15,
                            managed_cloud_worker_id = ?16,
                            managed_cloud_gateway_authority_base64url = ?17,
                            managed_cloud_gateway_authority_sha256 = ?18,
                            managed_cloud_lease_authority_sha256 = ?19
                      WHERE run_id = ?1 AND phase = 'prepared' AND fence = ?7",
                    params![
                        run_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                        existing.fence,
                        bound_managed_cloud.as_ref().map(|bound| bound
                            .authority
                            .managed_cloud_workflow_request_id
                            .as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.request_command_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.execution_command_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.binding_sha256.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_memo_base64url.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_sha256.as_str()),
                        bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_runtime_instance_id.as_str()
                        }),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_runtime_instance_epoch),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_worker_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_base64url.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_sha256.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.lease_authority_sha256.as_str()),
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                (fence, bound_managed_cloud)
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
                let bound_managed_cloud = managed_cloud_authority
                    .clone()
                    .map(|authority| {
                        bind_managed_cloud_execution_lease_authority(
                            authority,
                            run_id,
                            fence,
                            &lease_token_sha256,
                        )
                    })
                    .transpose()
                    .map_err(execution_lease_from_managed_cloud_error)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms,
                        managed_cloud_workflow_request_id,
                        managed_cloud_request_command_id,
                        managed_cloud_execution_command_id,
                        managed_cloud_binding_sha256,
                        managed_cloud_release_memo_base64url,
                        managed_cloud_release_sha256,
                        managed_cloud_runtime_instance_id,
                        managed_cloud_runtime_instance_epoch,
                        managed_cloud_worker_id,
                        managed_cloud_gateway_authority_base64url,
                        managed_cloud_gateway_authority_sha256,
                        managed_cloud_lease_authority_sha256
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', ?8, ?9, ?9,
                        ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
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
                        bound_managed_cloud.as_ref().map(|bound| bound
                            .authority
                            .managed_cloud_workflow_request_id
                            .as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.request_command_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.execution_command_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.binding_sha256.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_memo_base64url.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_sha256.as_str()),
                        bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_runtime_instance_id.as_str()
                        }),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_runtime_instance_epoch),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_worker_id.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_base64url.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_sha256.as_str()),
                        bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.lease_authority_sha256.as_str()),
                    ],
                ) {
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                (fence, bound_managed_cloud)
            };
            let bound_volume = match (volume_binding, prepared_binding.as_ref()) {
                (Some(binding), Some(prepared)) => Some(
                    finalize_runner_volume_lease_binding_sqlite_tx(&tx, binding, prepared)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            if let Some(runtime) = trusted_runtime.as_ref() {
                bind_execution_lease_process_runtime_sqlite_tx(&tx, run_id, fence, runtime, now)
                    .map_err(execution_lease_from_runner_volume_error)?;
            }
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
                trusted_runtime,
                bound_managed_cloud.map(|bound| bound.authority),
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if let Some(input) = managed_cloud_context.and_then(|context| context.input) {
                lock_managed_cloud_workflow_admission_postgres_tx(
                    &mut tx,
                    &input.managed_cloud_release.execution.admission.scope,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            } else {
                lock_operational_hold_shared_postgres_tx(&mut tx)
                    .map_err(execution_lease_from_operational_hold_error)?;
                lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                    .map_err(execution_lease_from_managed_cloud_error)?;
                lock_postgres_ats_certification(&mut tx)
                    .map_err(execution_lease_from_ats_certification_error)?;
            }
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let discovered_application =
                discover_postgres_execution_application(&mut tx, account_id, application_id)?;
            if !lock_current_execution_authority_postgres_after_prelock(
                &mut tx,
                account_id,
                &discovered_application,
            )? {
                return Err(ExecutionLeaseError::Conflict);
            }
            match (volume_binding, proposed_subject.as_deref()) {
                (Some(binding), Some(subject)) => {
                    prepare_runner_volume_lease_binding_postgres_tx(&mut tx, binding, subject)
                        .map_err(execution_lease_from_runner_volume_error)?;
                }
                (None, None) => {}
                _ => return Err(ExecutionLeaseError::Conflict),
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let execution_target =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            if execution_target.browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            if !job_application_snapshot_matches(
                &discovered_application,
                &execution_target.application,
            ) {
                return Err(ExecutionLeaseError::Conflict);
            }
            prelock_postgres_cloud_ats_phase_a_rows(
                &mut tx,
                account_id,
                &execution_target.application,
                run_id,
            )?;
            let entitled = tx
                .query_opt(
                    "SELECT cloud_browser FROM jobs_entitlements
                      WHERE account_id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .is_some_and(|row| row.get::<_, bool>(0));
            if !entitled {
                return Err(ExecutionLeaseError::Conflict);
            }
            prelock_postgres_cloud_attempt(&mut tx, account_id, application_id)?;
            let hold_context =
                operational_hold_context_for_application_postgres_tx_after_authority_prelock(
                    &mut tx,
                    account_id,
                    application_id,
                    &execution_target.employer_domain,
                    Some("cloud"),
                    None,
                    None,
                )
                .map_err(execution_lease_from_operational_hold_error)?;
            require_operational_capability_postgres_tx_after_authority_prelock(
                &mut tx,
                OperationalCapability::RunnerClaim,
                &hold_context,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            let browser_profile_id = execution_target.browser_profile_id;
            if let Some(context) = managed_cloud_context {
                let _preliminary_managed =
                    resolve_managed_cloud_execution_lease_claim_postgres_tx_after_prelock(
                        &mut tx,
                        account_id,
                        application_id,
                        run_id,
                        context.input,
                        context.authenticated_worker_id,
                        context.volume_worker_id,
                    )
                    .map_err(execution_lease_from_managed_cloud_error)?;
            }
            match (volume_binding, expected_runtime) {
                (Some(binding), Some((expected_grant_id, expected_runtime_sha256))) => {
                    let runtime = require_runner_process_runtime_postgres_tx(
                        &mut tx,
                        &binding.worker_id,
                        &binding.volume_id,
                        binding.enrollment_epoch,
                        &binding.process_instance_id,
                    )
                    .map_err(execution_lease_from_runner_volume_error)?;
                    if runtime.runtime_grant_id != expected_grant_id
                        || runtime.runtime_sha256 != expected_runtime_sha256
                    {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                }
                (Some(_), None) if cfg!(debug_assertions) && volume_authority.is_none() => {}
                (None, None) => {}
                _ => return Err(ExecutionLeaseError::Conflict),
            }
            if let Some(authority) = volume_authority {
                prelock_runner_volume_authority_use_postgres_tx(&mut tx, authority)
                    .map_err(execution_lease_from_runner_volume_error)?;
            }
            if tx
                .query_opt(
                    "SELECT id FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .is_none()
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            prelock_postgres_execution_lease_set(
                &mut tx,
                account_id,
                application_id,
                &browser_profile_id,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
                    &[&run_id],
                )?
                .map(execution_lease_from_pg_row);
            let managed_resolution = match managed_cloud_context {
                Some(context) => Some(
                    resolve_managed_cloud_execution_lease_claim_postgres_tx_after_prelock(
                        &mut tx,
                        account_id,
                        application_id,
                        run_id,
                        context.input,
                        context.authenticated_worker_id,
                        context.volume_worker_id,
                    )
                    .map_err(execution_lease_from_managed_cloud_error)?,
                ),
                None => None,
            };
            let now = match managed_resolution.as_ref() {
                Some(resolution) => resolution.db_time_ms,
                None => original_source_db_now_postgres(&mut tx)
                    .map_err(execution_lease_from_original_source_error)?,
            };
            let current = resolve_current_execution_authority_postgres_after_prelock_at_ms(
                &mut tx,
                account_id,
                &execution_target.application,
                ExecutionAuthorityRunner::Cloud,
                now,
            )?;
            let current_employer_domain = current
                .authorized
                .then_some(current.employer_domain)
                .flatten()
                .ok_or(ExecutionLeaseError::Conflict)?;
            if current_employer_domain != execution_target.employer_domain {
                return Err(ExecutionLeaseError::Conflict);
            }
            let final_hold_context =
                operational_hold_context_for_application_postgres_tx_after_authority_prelock(
                    &mut tx,
                    account_id,
                    application_id,
                    &current_employer_domain,
                    Some("cloud"),
                    None,
                    None,
                )
                .map_err(execution_lease_from_operational_hold_error)?;
            require_operational_capability_postgres_tx_after_authority_prelock(
                &mut tx,
                OperationalCapability::RunnerClaim,
                &final_hold_context,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            let final_volume_binding = volume_binding.cloned().map(|mut binding| {
                binding.now_ms = now;
                binding
            });
            let prepared_binding =
                match (final_volume_binding.as_ref(), proposed_subject.as_deref()) {
                    (Some(binding), Some(subject)) => Some(
                        prepare_runner_volume_lease_binding_postgres_tx(&mut tx, binding, subject)
                            .map_err(execution_lease_from_runner_volume_error)?,
                    ),
                    (None, None) => None,
                    _ => return Err(ExecutionLeaseError::Conflict),
                };
            let trusted_runtime = match (final_volume_binding.as_ref(), expected_runtime) {
                (Some(binding), Some((expected_grant_id, expected_runtime_sha256))) => {
                    let runtime = require_runner_process_runtime_postgres_tx(
                        &mut tx,
                        &binding.worker_id,
                        &binding.volume_id,
                        binding.enrollment_epoch,
                        &binding.process_instance_id,
                    )
                    .map_err(execution_lease_from_runner_volume_error)?;
                    if runtime.runtime_grant_id != expected_grant_id
                        || runtime.runtime_sha256 != expected_runtime_sha256
                    {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    Some(runtime)
                }
                (Some(_), None) if cfg!(debug_assertions) && volume_authority.is_none() => None,
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            let managed_cloud_authority =
                managed_resolution.and_then(|resolution| resolution.authority);
            let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
            if let (Some(binding), Some(authority)) =
                (final_volume_binding.as_ref(), volume_authority)
            {
                consume_runner_volume_authority_postgres_tx(
                    &mut tx,
                    authority,
                    "execution_lease_claim",
                    &binding.volume_id,
                    binding.enrollment_epoch,
                    &binding.process_instance_id,
                    now,
                )
                .map_err(execution_lease_from_runner_volume_error)?;
            }
            postgres_bind_cloud_attempt(&mut tx, account_id, application_id, now)?;
            create_cloud_ats_phase_a_postgres_tx(
                &mut tx,
                &CloudAtsPhaseAContext {
                    account_id,
                    application_id,
                    run_id,
                    browser_profile_id: &browser_profile_id,
                    trusted_runtime: trusted_runtime.as_ref(),
                    nonce_sha256: &lease_token_sha256,
                    now_ms: now,
                },
            )?;
            let (fence, bound_managed_cloud) = if let Some(existing) = existing {
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
                let bound_managed_cloud = managed_cloud_authority
                    .clone()
                    .map(|authority| {
                        bind_managed_cloud_execution_lease_authority(
                            authority,
                            run_id,
                            fence,
                            &lease_token_sha256,
                        )
                    })
                    .transpose()
                    .map_err(execution_lease_from_managed_cloud_error)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = $2, lease_token_sha256 = $3, fence = $4,
                            lease_expires_at_ms = $5, updated_at_ms = $6,
                            managed_cloud_workflow_request_id = $8,
                            managed_cloud_request_command_id = $9,
                            managed_cloud_execution_command_id = $10,
                            managed_cloud_binding_sha256 = $11,
                            managed_cloud_release_memo_base64url = $12,
                            managed_cloud_release_sha256 = $13,
                            managed_cloud_runtime_instance_id = $14,
                            managed_cloud_runtime_instance_epoch = $15,
                            managed_cloud_worker_id = $16,
                            managed_cloud_gateway_authority_base64url = $17,
                            managed_cloud_gateway_authority_sha256 = $18,
                            managed_cloud_lease_authority_sha256 = $19
                      WHERE run_id = $1 AND phase = 'prepared' AND fence = $7",
                    &[
                        &run_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                        &existing.fence,
                        &bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_workflow_request_id.as_str()
                        }),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.request_command_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.execution_command_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.binding_sha256.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_memo_base64url.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_sha256.as_str()),
                        &bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_runtime_instance_id.as_str()
                        }),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_runtime_instance_epoch),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_worker_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_base64url.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_sha256.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.lease_authority_sha256.as_str()),
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                (fence, bound_managed_cloud)
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
                let bound_managed_cloud = managed_cloud_authority
                    .clone()
                    .map(|authority| {
                        bind_managed_cloud_execution_lease_authority(
                            authority,
                            run_id,
                            fence,
                            &lease_token_sha256,
                        )
                    })
                    .transpose()
                    .map_err(execution_lease_from_managed_cloud_error)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms,
                        managed_cloud_workflow_request_id,
                        managed_cloud_request_command_id,
                        managed_cloud_execution_command_id,
                        managed_cloud_binding_sha256,
                        managed_cloud_release_memo_base64url,
                        managed_cloud_release_sha256,
                        managed_cloud_runtime_instance_id,
                        managed_cloud_runtime_instance_epoch,
                        managed_cloud_worker_id,
                        managed_cloud_gateway_authority_base64url,
                        managed_cloud_gateway_authority_sha256,
                        managed_cloud_lease_authority_sha256
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'prepared', $8, $9, $9,
                        $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)",
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
                        &bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_workflow_request_id.as_str()
                        }),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.request_command_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.execution_command_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.binding_sha256.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_memo_base64url.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.release_sha256.as_str()),
                        &bound_managed_cloud.as_ref().map(|bound| {
                            bound.authority.managed_cloud_runtime_instance_id.as_str()
                        }),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_runtime_instance_epoch),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.managed_cloud_worker_id.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_base64url.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.authority.gateway_authority_sha256.as_str()),
                        &bound_managed_cloud
                            .as_ref()
                            .map(|bound| bound.lease_authority_sha256.as_str()),
                    ],
                ) {
                    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                (fence, bound_managed_cloud)
            };
            let bound_volume = match (final_volume_binding.as_ref(), prepared_binding.as_ref()) {
                (Some(binding), Some(prepared)) => Some(
                    finalize_runner_volume_lease_binding_postgres_tx(&mut tx, binding, prepared)
                        .map_err(execution_lease_from_runner_volume_error)?,
                ),
                (None, None) => None,
                _ => return Err(ExecutionLeaseError::Conflict),
            };
            if let Some(runtime) = trusted_runtime.as_ref() {
                bind_execution_lease_process_runtime_postgres_tx(
                    &mut tx, run_id, fence, runtime, now,
                )
                .map_err(execution_lease_from_runner_volume_error)?;
            }
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
                trusted_runtime,
                bound_managed_cloud.map(|bound| bound.authority),
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

fn execution_lease_from_ats_certification_error(
    error: AtsCertificationAuthorityError,
) -> ExecutionLeaseError {
    match error {
        AtsCertificationAuthorityError::Storage(error) => ExecutionLeaseError::Storage(error),
        _ => ExecutionLeaseError::Conflict,
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

fn managed_execution_volume_worker_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    authenticated_worker_id: &str,
    now: i64,
) -> ExecutionLeaseResult<String> {
    require_current_runner_volume_identity_sqlite_for_operation(tx, account_id, run_id, now)?;
    let volume_worker_id = tx
        .query_row(
            "SELECT volume.worker_id
               FROM jobs_execution_lease_volume_bindings binding
               JOIN jobs_runner_volumes volume
                 ON volume.volume_id = binding.volume_id
                AND volume.current_epoch = binding.volume_epoch
                AND volume.active_instance_id = binding.process_instance_id
              WHERE binding.run_id = ?1 AND volume.status = 'active'
                AND volume.instance_lease_expires_at_ms > ?2",
            params![run_id, now],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::Conflict)?;
    if volume_worker_id != authenticated_worker_id {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(volume_worker_id)
}

fn discover_managed_execution_volume_worker_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    authenticated_worker_id: &str,
) -> ExecutionLeaseResult<String> {
    let volume_worker_id = tx
        .query_opt(
            "SELECT volume.worker_id
               FROM jobs_execution_lease_volume_bindings binding
               JOIN jobs_runner_volumes volume ON volume.volume_id = binding.volume_id
              WHERE binding.run_id = $1",
            &[&run_id],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?
        .get::<_, String>(0);
    if volume_worker_id != authenticated_worker_id {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(volume_worker_id)
}

fn revalidate_managed_execution_volume_worker_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    authenticated_worker_id: &str,
    expected_volume_worker_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let row = tx
        .query_opt(
            "SELECT volume.worker_id, subject.purge_subject,
                    binding.purge_subject_sha256
               FROM jobs_execution_lease_volume_bindings binding
               JOIN jobs_runner_volumes volume
                 ON volume.volume_id = binding.volume_id
                AND volume.current_epoch = binding.volume_epoch
                AND volume.active_instance_id = binding.process_instance_id
               JOIN jobs_runner_volume_keys volume_key
                 ON volume_key.volume_id = volume.volume_id
                AND volume_key.enrollment_epoch = volume.current_epoch
               JOIN jobs_runner_account_subjects subject
                 ON subject.account_id = $2
               JOIN jobs_runner_volume_residencies residency
                 ON residency.purge_subject = subject.purge_subject
                AND residency.volume_id = volume.volume_id
                AND residency.volume_epoch = volume.current_epoch
              WHERE binding.run_id = $1 AND volume.status = 'active'
                AND volume.instance_lease_expires_at_ms > $3
                AND volume.required_tombstone_generation =
                    volume.reconciled_tombstone_generation
                AND volume_key.retired_at_ms IS NULL
                AND subject.legacy_unresolved = FALSE
                AND residency.state = 'resident' AND residency.purge_generation = 0
                AND NOT EXISTS (
                    SELECT 1 FROM jobs_runner_purge_requests purge_request
                     WHERE purge_request.account_id = $2
                        OR purge_request.purge_subject = subject.purge_subject)
                AND NOT EXISTS (
                    SELECT 1 FROM jobs_runner_purge_tombstones tombstone
                     WHERE tombstone.purge_subject = subject.purge_subject)
                AND EXISTS (
                    SELECT 1 FROM jobs_runner_volume_storage_attestations attestation
                     WHERE attestation.volume_id = volume.volume_id
                       AND attestation.enrollment_epoch = volume.current_epoch
                       AND attestation.volume_key_fingerprint = volume_key.key_fingerprint
                       AND attestation.process_instance_id = volume.active_instance_id
                       AND attestation.enrollment_generation = volume.enrollment_generation
                       AND attestation.required_tombstone_generation =
                           volume.required_tombstone_generation
                       AND attestation.reconciled_tombstone_generation =
                           volume.reconciled_tombstone_generation
                       AND NOT EXISTS (
                           SELECT 1 FROM jobs_runner_volume_storage_attestations newer
                            WHERE newer.volume_id = attestation.volume_id
                              AND newer.enrollment_epoch = attestation.enrollment_epoch
                              AND newer.attestation_generation >
                                  attestation.attestation_generation))
              FOR SHARE OF binding, volume, volume_key, subject, residency",
            &[&run_id, &account_id, &now],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?;
    let volume_worker_id: String = row.get(0);
    let purge_subject: String = row.get(1);
    let purge_subject_sha256: String = row.get(2);
    if volume_worker_id != authenticated_worker_id
        || volume_worker_id != expected_volume_worker_id
        || runner_purge_subject_sha256(&purge_subject)
            .map_err(execution_lease_from_runner_volume_error)?
            != purge_subject_sha256
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
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
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            let now = original_source_db_now_sqlite(&tx)
                .map_err(execution_lease_from_original_source_error)?;
            let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
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
            // This first pass only acquires and checks the runner-volume rows. The result never
            // authorizes a renewal; every temporal check is repeated after the exact lease lock.
            let preliminary_now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx,
                account_id,
                run_id,
                preliminary_now,
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
            let now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
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

#[allow(clippy::too_many_arguments)]
pub fn authorize_managed_execution_effect(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    managed_cloud: &ManagedCloudExecutionLeaseClaimInput,
    authenticated_worker_id: &str,
) -> ExecutionLeaseResult<AuthorizedExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    if !validate_execution_binding(authenticated_worker_id, 240) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let lease_token_sha256 = execution_lease_token_hash(lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let execution_target =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            let preliminary_now = original_source_db_now_sqlite(&tx)
                .map_err(execution_lease_from_original_source_error)?;
            managed_execution_volume_worker_sqlite_tx(
                &tx,
                account_id,
                run_id,
                authenticated_worker_id,
                preliminary_now,
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
            let now = original_source_db_now_sqlite(&tx)
                .map_err(execution_lease_from_original_source_error)?;
            let current = resolve_current_execution_authority_sqlite_after_prelock_at_ms(
                &tx,
                account_id,
                &execution_target.application,
                ExecutionAuthorityRunner::Cloud,
                now,
            )?;
            if !current.authorized {
                return Err(ExecutionLeaseError::Conflict);
            }
            let volume_worker_id = managed_execution_volume_worker_sqlite_tx(
                &tx,
                account_id,
                run_id,
                authenticated_worker_id,
                now,
            )?;
            let managed_cloud = resolve_managed_cloud_execution_effect_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                fence,
                &lease_token_sha256,
                Some(managed_cloud),
                authenticated_worker_id,
                &volume_worker_id,
            )
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            if lease.fence != fence
                || lease.lease_token_sha256 != lease_token_sha256
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let record = AuthorizedExecutionLeaseRecord {
                lease: ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: lease.phase,
                },
                managed_cloud: Some(managed_cloud),
            };
            tx.commit()?;
            Ok(record)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_managed_cloud_workflow_admission_postgres_tx(
                &mut tx,
                &managed_cloud
                    .managed_cloud_release
                    .execution
                    .admission
                    .scope,
            )
            .map_err(execution_lease_from_managed_cloud_error)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let volume_worker_id = discover_managed_execution_volume_worker_postgres_tx(
                &mut tx,
                run_id,
                authenticated_worker_id,
            )?;
            resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(
                &mut tx,
                account_id,
                application_id,
                run_id,
                fence,
                &lease_token_sha256,
                Some(managed_cloud),
                authenticated_worker_id,
                &volume_worker_id,
            )
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?;
            let preliminary_now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            revalidate_managed_execution_volume_worker_postgres_tx(
                &mut tx,
                account_id,
                run_id,
                authenticated_worker_id,
                &volume_worker_id,
                preliminary_now,
            )?;
            let managed_resolution =
                resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    Some(managed_cloud),
                    authenticated_worker_id,
                    &volume_worker_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?
                .ok_or(ExecutionLeaseError::Conflict)?;
            let now = managed_resolution.db_time_ms;
            let managed_cloud = managed_resolution.authority;
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
                || lease.lease_token_sha256 != lease_token_sha256
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            revalidate_managed_execution_volume_worker_postgres_tx(
                &mut tx,
                account_id,
                run_id,
                authenticated_worker_id,
                &volume_worker_id,
                now,
            )?;
            let record = AuthorizedExecutionLeaseRecord {
                lease: ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: lease.phase,
                },
                managed_cloud: Some(managed_cloud),
            };
            tx.commit()?;
            Ok(record)
        }
    })
}

#[cfg(test)]
type ManagedExecutionEffectBoundary = fn(
    &DbPool,
    &str,
    &str,
    &str,
    &str,
    i64,
    &ManagedCloudExecutionLeaseClaimInput,
    &str,
) -> ExecutionLeaseResult<AuthorizedExecutionLeaseRecord>;

#[cfg(test)]
type IrreversibleSubmitWorkerBoundary = fn(
    &DbPool,
    &str,
    &str,
    &str,
    &str,
    i64,
    &FinalSubmitProof,
    &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    Option<&ManagedCloudExecutionLeaseClaimInput>,
    &str,
) -> ExecutionLeaseResult<AuthorizedIrreversibleExecutionLeaseRecord>;

#[cfg(test)]
#[test]
fn managed_execution_effect_boundary_requires_authority_in_type() {
    let boundary: ManagedExecutionEffectBoundary = authorize_managed_execution_effect;
    let _ = boundary;
}

#[cfg(test)]
#[test]
fn irreversible_submit_worker_boundary_preserves_optional_input_for_authoritative_pairing() {
    let boundary: IrreversibleSubmitWorkerBoundary = start_irreversible_submission_authorized;
    let _ = boundary;

    let source = include_str!("execution_leases.rs");
    let effect = source
        .rsplit("fn start_irreversible_submission_inner(")
        .next()
        .expect("irreversible-submit implementation")
        .split("fn execution_finish_allowed(")
        .next()
        .expect("bounded irreversible-submit implementation");
    assert!(effect.contains("require_unmanaged_cloud_execution_sqlite_tx("));
    assert!(effect.contains("require_unmanaged_cloud_execution_postgres_tx("));
    assert_eq!(effect.matches("unwrap_or((None, \"\"))").count(), 2);
    assert!(effect.contains("load_managed_cloud_irreversible_effect_receipt_sqlite_tx("));
    assert!(effect.contains("load_managed_cloud_irreversible_effect_receipt_postgres_tx("));
}

#[cfg(test)]
#[test]
fn managed_execution_effect_rechecks_composed_authority_after_canonical_prelock() {
    let source = include_str!("execution_leases.rs");
    let section = source
        .split("pub fn authorize_managed_execution_effect(")
        .nth(1)
        .expect("managed execution effect boundary")
        .split("\n#[cfg(test)]")
        .next()
        .expect("bounded managed execution effect boundary");
    let mut previous = 0;
    for operation in [
        "lock_managed_cloud_workflow_admission_postgres_tx",
        "lock_discovery_account_shared_postgres",
        "resolve_managed_cloud_execution_effect_postgres_tx_after_prelock",
    ] {
        let position = section
            .find(operation)
            .unwrap_or_else(|| panic!("missing managed effect operation {operation}"));
        assert!(
            position >= previous,
            "managed effect order inverted at {operation}"
        );
        previous = position;
    }
    assert!(!section.contains("current_execution_authorized_postgres("));

    let managed_source = include_str!("managed_cloud_release_authority.rs");
    let prelock = managed_source
        .split("fn prelock_postgres_managed_cloud_effect_admission_inputs(")
        .nth(1)
        .expect("managed effect application/entitlement prelock")
        .split("fn require_postgres_managed_cloud_effect_admission_after_full_prelock_at_ms(")
        .next()
        .expect("bounded managed effect application/entitlement prelock");
    assert!(prelock.contains("lock_current_execution_authority_postgres_after_prelock"));

    let resolver = managed_source
        .split("pub(crate) fn resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(")
        .nth(1)
        .expect("managed effect PostgreSQL resolver")
        .split("const MANAGED_CLOUD_IRREVERSIBLE_RECEIPT_COLUMNS")
        .next()
        .expect("bounded managed effect PostgreSQL resolver");
    let final_time = resolver
        .rfind("let now_ms = managed_cloud_db_now_postgres")
        .expect("managed effect final database time");
    assert!(resolver[final_time..]
        .contains("require_postgres_managed_cloud_effect_admission_after_full_prelock_at_ms"));
}

#[cfg(test)]
#[test]
fn fix_728_cloud_claim_and_submit_hold_checks_precede_effect_mutation_without_relocks() {
    let source = include_str!("execution_leases.rs");
    let claim = source
        .split("fn claim_execution_lease_inner(")
        .nth(1)
        .expect("cloud claim implementation")
        .split("\nfn execution_lease_from_runner_volume_error(")
        .next()
        .expect("bounded cloud claim implementation")
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL cloud claim implementation");
    let mut previous = 0;
    for operation in [
        "lock_discovery_account_shared_postgres",
        "postgres_execution_target",
        "operational_hold_context_for_application_postgres_tx_after_authority_prelock",
        "require_operational_capability_postgres_tx_after_authority_prelock",
        "prepare_runner_volume_lease_binding_postgres_tx",
        "postgres_bind_cloud_attempt",
        "create_cloud_ats_phase_a_postgres_tx",
    ] {
        let relative = claim[previous..]
            .find(operation)
            .unwrap_or_else(|| panic!("missing cloud claim operation {operation}"));
        previous += relative + operation.len();
    }
    for forbidden in [
        "operational_hold_context_for_application_postgres_tx(",
        "require_operational_capability_postgres_tx(",
        "create_ats_application_certification_binding_from_context_postgres_tx(",
    ] {
        assert!(
            !claim.contains(forbidden),
            "cloud claim uses legacy {forbidden}"
        );
    }
    let phase_a = source
        .split("fn create_cloud_ats_phase_a_postgres_tx(")
        .nth(1)
        .expect("PostgreSQL cloud Phase A helper")
        .split("fn claim_execution_lease_inner(")
        .next()
        .expect("bounded PostgreSQL cloud Phase A helper");
    assert!(phase_a.contains(
        "create_ats_application_certification_binding_from_context_postgres_tx_after_prelock"
    ));
    assert!(
        !phase_a.contains("create_ats_application_certification_binding_from_context_postgres_tx(")
    );

    let submit = source
        .rsplit("fn start_irreversible_submission_inner(")
        .next()
        .expect("cloud final-submit implementation")
        .split("pub fn finish_execution_lease(")
        .next()
        .expect("bounded cloud final-submit implementation")
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL cloud final-submit implementation");
    let mut previous = 0;
    for operation in [
        "lock_discovery_account_shared_postgres",
        "postgres_execution_target",
        "operational_hold_context_for_application_postgres_tx_after_authority_prelock",
        "require_operational_capability_postgres_tx_after_authority_prelock",
        "resolve_managed_cloud_execution_effect_postgres_tx_after_prelock",
        "FROM jobs_execution_leases",
        "validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock",
        "bind_final_submit_proof_postgres_tx",
        "reserve_submission_evidence_capacity_postgres_tx",
        "SET phase = 'click_started'",
    ] {
        let relative = submit[previous..]
            .find(operation)
            .unwrap_or_else(|| panic!("missing cloud submit operation {operation}"));
        previous += relative + operation.len();
    }
    for forbidden in [
        "operational_hold_context_for_application_postgres_tx(",
        "require_operational_capability_postgres_tx(",
        "validate_consume_reserve_ats_application_certification_from_context_postgres_tx(",
    ] {
        assert!(
            !submit.contains(forbidden),
            "cloud submit uses legacy {forbidden}"
        );
    }
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

fn submission_evidence_capacity_at_ms(
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now_ms: i64,
) -> crate::db::object_uploads::NewSubmissionEvidenceCapacity {
    let mut capacity = capacity.clone();
    capacity.now_ms = now_ms;
    capacity.expires_at_ms = now_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
    capacity
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

fn irreversible_execution_lease_record(
    run_id: &str,
    fence: i64,
    lease_expires_at_ms: i64,
    ats_certified_receipt_authority: Option<AtsCertifiedReceiptAuthority>,
) -> IrreversibleExecutionLeaseRecord {
    IrreversibleExecutionLeaseRecord {
        lease: ExecutionLeaseRecord {
            run_id: run_id.to_string(),
            fence,
            lease_expires_at_ms,
            phase: "click_started".to_string(),
        },
        ats_certified_receipt_authority,
    }
}

fn require_exact_stored_final_submit_proof_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    proof: &FinalSubmitProof,
) -> ExecutionLeaseResult<()> {
    let (job_id, application_json): (String, String) = tx
        .query_row(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        application_json,
        application_id,
        &job_id,
        "irreversible ATS recovery application",
    )?;
    let presented = serde_json::to_value(proof).map_err(anyhow::Error::from)?;
    if application.receipt.get(FINAL_SUBMIT_PROOF_KEY) != Some(&presented) {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn require_exact_stored_final_submit_proof_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    proof: &FinalSubmitProof,
) -> ExecutionLeaseResult<()> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        application_id,
        &job_id,
        "irreversible ATS recovery application",
    )?;
    let presented = serde_json::to_value(proof).map_err(anyhow::Error::from)?;
    if application.receipt.get(FINAL_SUBMIT_PROOF_KEY) != Some(&presented) {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn ats_phase_b_context(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    proof: &FinalSubmitProof,
) -> ExecutionLeaseResult<Option<AtsCertificationPhaseBContextRequest>> {
    if proof.schema_version != 4 {
        return Ok(None);
    }
    let observed = proof
        .observed_surface
        .as_ref()
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    Ok(Some(AtsCertificationPhaseBContextRequest {
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        runner_kind: "cloud".to_string(),
        observed_surface: AtsObservedSurface {
            variant_key: observed.variant_key.clone(),
            layout_contract_version: observed.layout_contract_version,
            surface_sha256: observed.surface_sha256.clone(),
        },
        terminal_phase: "consumed".to_string(),
    }))
}

fn recover_terminal_ats_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<AtsCertifiedReceiptAuthority> {
    let mut stmt = tx.prepare(
        "SELECT binding_id, attempt_id, nonce_sha256
           FROM jobs_application_ats_certification_bindings
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND phase IN ('consumed', 'side_effect_unknown') LIMIT 2",
    )?;
    let rows = stmt
        .query_map(params![account_id, application_id, run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    let (binding_id, application_attempt_id, nonce_sha256) = rows
        .into_iter()
        .next()
        .ok_or(ExecutionLeaseError::Conflict)?;
    recover_ats_application_certification_sqlite_tx(
        tx,
        &AtsCertificationRecoveryRequest {
            binding_id,
            account_id: account_id.to_string(),
            application_id: application_id.to_string(),
            run_id: run_id.to_string(),
            application_attempt_id,
            nonce_sha256,
        },
    )
    .map(|result| result.ats_certified_receipt_authority)
    .map_err(execution_lease_from_ats_certification_error)
}

fn recover_terminal_ats_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<AtsCertifiedReceiptAuthority> {
    let rows = tx.query(
        "SELECT binding_id, attempt_id, nonce_sha256
           FROM jobs_application_ats_certification_bindings
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND phase IN ('consumed', 'side_effect_unknown') LIMIT 2 FOR UPDATE",
        &[&account_id, &application_id, &run_id],
    )?;
    if rows.len() != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    let row = &rows[0];
    recover_ats_application_certification_postgres_tx(
        tx,
        &AtsCertificationRecoveryRequest {
            binding_id: row.get(0),
            account_id: account_id.to_string(),
            application_id: application_id.to_string(),
            run_id: run_id.to_string(),
            application_attempt_id: row.get(1),
            nonce_sha256: row.get(2),
        },
    )
    .map(|result| result.ats_certified_receipt_authority)
    .map_err(execution_lease_from_ats_certification_error)
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
) -> ExecutionLeaseResult<IrreversibleExecutionLeaseRecord> {
    start_irreversible_submission_inner(
        pool,
        account_id,
        application_id,
        run_id,
        lease_token,
        fence,
        final_submit_proof,
        capacity,
        None,
    )
    .map(|record| record.record)
}

#[allow(clippy::too_many_arguments)]
pub fn start_irreversible_submission_authorized(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    managed_cloud: Option<&ManagedCloudExecutionLeaseClaimInput>,
    authenticated_worker_id: &str,
) -> ExecutionLeaseResult<AuthorizedIrreversibleExecutionLeaseRecord> {
    start_irreversible_submission_inner(
        pool,
        account_id,
        application_id,
        run_id,
        lease_token,
        fence,
        final_submit_proof,
        capacity,
        Some((managed_cloud, authenticated_worker_id)),
    )
}

#[allow(clippy::too_many_arguments)]
fn start_irreversible_submission_inner(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    managed_cloud_context: Option<(Option<&ManagedCloudExecutionLeaseClaimInput>, &str)>,
) -> ExecutionLeaseResult<AuthorizedIrreversibleExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    if managed_cloud_context
        .is_some_and(|(_, worker_id)| !validate_execution_binding(worker_id, 240))
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let lease_token_sha256 = execution_lease_token_hash(lease_token);
    // This wall-clock validation is input-shape/early-denial only. Each database branch repeats
    // the complete capacity and execution validation using a post-lock database scalar.
    let preliminary_now = now_ms();
    validate_submission_evidence_capacity_binding(
        account_id,
        application_id,
        run_id,
        capacity,
        preliminary_now,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            let now = original_source_db_now_sqlite(&tx)
                .map_err(execution_lease_from_original_source_error)?;
            let capacity = submission_evidence_capacity_at_ms(capacity, now);
            validate_submission_evidence_capacity_binding(
                account_id,
                application_id,
                run_id,
                &capacity,
                now,
            )?;
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == "click_started" {
                let (input, authenticated_worker_id) =
                    managed_cloud_context.unwrap_or((None, ""));
                let managed_cloud = load_managed_cloud_irreversible_effect_receipt_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    input,
                    authenticated_worker_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?
                .map(|receipt| receipt.authority);
                if final_submit_proof.schema_version != 4 {
                    return Err(ExecutionLeaseError::Conflict);
                }
                require_exact_stored_final_submit_proof_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    final_submit_proof,
                )?;
                let authority = recover_terminal_ats_authority_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    run_id,
                )?;
                let record = irreversible_execution_lease_record(
                    run_id,
                    fence,
                    lease.lease_expires_at_ms,
                    Some(authority),
                );
                tx.commit()?;
                return Ok(AuthorizedIrreversibleExecutionLeaseRecord {
                    record,
                    managed_cloud,
                });
            }
            if managed_cloud_context
                .and_then(|(input, _)| input)
                .is_none()
            {
                require_unmanaged_cloud_execution_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    run_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            }
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let execution_target =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            if lease.browser_profile_id != execution_target.browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            let hold_context = operational_hold_context_for_application_sqlite_tx_after_authority(
                &tx,
                account_id,
                application_id,
                &execution_target.employer_domain,
                Some("cloud"),
                None,
                None,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            require_operational_capability_sqlite_tx(
                &tx,
                OperationalCapability::FinalSubmit,
                &hold_context,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            let managed_cloud = match managed_cloud_context {
                Some((input, authenticated_worker_id)) => {
                    let volume_worker_id = match input {
                        Some(_) => managed_execution_volume_worker_sqlite_tx(
                            &tx,
                            account_id,
                            run_id,
                            authenticated_worker_id,
                            now,
                        )?,
                        None => authenticated_worker_id.to_string(),
                    };
                    resolve_managed_cloud_execution_effect_sqlite_tx(
                        &tx,
                        account_id,
                        application_id,
                        run_id,
                        fence,
                        &lease_token_sha256,
                        input,
                        authenticated_worker_id,
                        &volume_worker_id,
                    )
                    .map_err(execution_lease_from_managed_cloud_error)?
                }
                None => None,
            };
            if lease.phase != "prepared" || lease.lease_expires_at_ms <= now {
                return Err(ExecutionLeaseError::Conflict);
            }
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            let ats_certified_receipt_authority = match ats_phase_b_context(
                account_id,
                application_id,
                run_id,
                final_submit_proof,
            )? {
                Some(request) => match
                    validate_consume_reserve_ats_application_certification_from_context_sqlite_tx(
                        &tx, &request, now,
                    )
                    .map_err(execution_lease_from_ats_certification_error)?
                {
                    AtsCertificationPhaseBTransactionOutcome::Authorized(result) => {
                        Some(result.ats_certified_receipt_authority)
                    }
                    AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined => {
                        tx.commit()?;
                        return Err(ExecutionLeaseError::Conflict);
                    }
                },
                None => None,
            };
            bind_final_submit_proof_sqlite_tx(
                &tx,
                account_id,
                application_id,
                final_submit_proof,
                now,
            )?;
            crate::db::object_uploads::reserve_submission_evidence_capacity_sqlite_tx(
                &tx, &capacity,
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
            if let Some(authority) = managed_cloud.as_ref() {
                insert_managed_cloud_irreversible_effect_receipt_sqlite_tx(
                    &tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    authority,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(AuthorizedIrreversibleExecutionLeaseRecord {
                record: irreversible_execution_lease_record(
                    run_id,
                    fence,
                    lease_expires_at_ms,
                    ats_certified_receipt_authority,
                ),
                managed_cloud,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let discovered_lease = tx
                .query_opt(
                    "SELECT phase, lease_expires_at_ms FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                        AND fence = $4 AND lease_token_sha256 = $5",
                    &[
                        &run_id,
                        &account_id,
                        &application_id,
                        &fence,
                        &lease_token_sha256,
                    ],
                )?
                .map(|row| (row.get::<_, String>(0), row.get::<_, i64>(1)));
            if discovered_lease
                .as_ref()
                .is_some_and(|(phase, _)| phase == "click_started")
            {
                lock_postgres_ats_certification(&mut tx)
                    .map_err(execution_lease_from_ats_certification_error)?;
                let (input, authenticated_worker_id) =
                    managed_cloud_context.unwrap_or((None, ""));
                let managed_cloud = load_managed_cloud_irreversible_effect_receipt_postgres_tx(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    input,
                    authenticated_worker_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?
                .map(|receipt| receipt.authority);
                let lease_expires_at_ms: i64 = tx
                    .query_opt(
                        "SELECT lease_expires_at_ms FROM jobs_execution_leases
                          WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                            AND fence = $4 AND lease_token_sha256 = $5
                            AND phase = 'click_started' FOR SHARE",
                        &[
                            &run_id,
                            &account_id,
                            &application_id,
                            &fence,
                            &lease_token_sha256,
                        ],
                    )?
                    .ok_or(ExecutionLeaseError::NotFound)?
                    .get(0);
                if final_submit_proof.schema_version != 4 {
                    return Err(ExecutionLeaseError::Conflict);
                }
                require_exact_stored_final_submit_proof_postgres_tx(
                    &mut tx,
                    account_id,
                    application_id,
                    final_submit_proof,
                )?;
                let authority = recover_terminal_ats_authority_postgres_tx(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                )?;
                let record = AuthorizedIrreversibleExecutionLeaseRecord {
                    record: irreversible_execution_lease_record(
                        run_id,
                        fence,
                        lease_expires_at_ms,
                        Some(authority),
                    ),
                    managed_cloud,
                };
                tx.commit()?;
                return Ok(record);
            }
            if let Some((Some(input), _)) = managed_cloud_context {
                lock_managed_cloud_workflow_admission_postgres_tx(
                    &mut tx,
                    &input.managed_cloud_release.execution.admission.scope,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            } else {
                lock_operational_hold_shared_postgres_tx(&mut tx)
                    .map_err(execution_lease_from_operational_hold_error)?;
                lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                    .map_err(execution_lease_from_managed_cloud_error)?;
                lock_postgres_ats_certification(&mut tx)
                    .map_err(execution_lease_from_ats_certification_error)?;
            }
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            if managed_cloud_context
                .and_then(|(input, _)| input)
                .is_none()
            {
                require_unmanaged_cloud_execution_postgres_tx(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            }
            let discovered_application =
                discover_postgres_execution_application(&mut tx, account_id, application_id)?;
            if !lock_current_execution_authority_postgres_after_prelock(
                &mut tx,
                account_id,
                &discovered_application,
            )? {
                return Err(ExecutionLeaseError::Conflict);
            }
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx,
                account_id,
                run_id,
                preliminary_now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let execution_target =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            if !job_application_snapshot_matches(
                &discovered_application,
                &execution_target.application,
            ) {
                return Err(ExecutionLeaseError::Conflict);
            }
            let entitled = tx
                .query_opt(
                    "SELECT cloud_browser FROM jobs_entitlements
                      WHERE account_id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .is_some_and(|row| row.get::<_, bool>(0));
            if !entitled {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx
                .query_opt(
                    "SELECT application_id FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .is_none()
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let hold_context =
                operational_hold_context_for_application_postgres_tx_after_authority_prelock(
                    &mut tx,
                    account_id,
                    application_id,
                    &execution_target.employer_domain,
                    Some("cloud"),
                    None,
                    None,
                )
                .map_err(execution_lease_from_operational_hold_error)?;
            require_operational_capability_postgres_tx_after_authority_prelock(
                &mut tx,
                OperationalCapability::FinalSubmit,
                &hold_context,
            )
            .map_err(execution_lease_from_operational_hold_error)?;
            let managed_worker = match managed_cloud_context {
                Some((Some(input), authenticated_worker_id)) => Some((
                    input,
                    authenticated_worker_id,
                    discover_managed_execution_volume_worker_postgres_tx(
                        &mut tx,
                        run_id,
                        authenticated_worker_id,
                    )?,
                )),
                _ => None,
            };
            let preliminary_now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            if let Some((input, authenticated_worker_id, volume_worker_id)) =
                managed_worker.as_ref()
            {
                revalidate_managed_execution_volume_worker_postgres_tx(
                    &mut tx,
                    account_id,
                    run_id,
                    authenticated_worker_id,
                    volume_worker_id,
                    preliminary_now,
                )?;
                resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    Some(*input),
                    authenticated_worker_id,
                    volume_worker_id,
                )
                .map_err(execution_lease_from_managed_cloud_error)?
                .ok_or(ExecutionLeaseError::Conflict)?;
            } else {
                require_current_runner_volume_binding_postgres_for_operation(
                    &mut tx,
                    account_id,
                    run_id,
                    preliminary_now,
                )?;
            }
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
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx
                .query_opt(
                    "SELECT id FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .is_none()
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.query(
                "SELECT application_id, run_id FROM jobs_submission_evidence_capacity
                  WHERE account_id = $1 ORDER BY application_id, run_id FOR UPDATE",
                &[&account_id],
            )?;
            let (managed_cloud, now) =
                if let Some((input, authenticated_worker_id, volume_worker_id)) =
                    managed_worker.as_ref()
                {
                    let resolution =
                        resolve_managed_cloud_execution_effect_postgres_tx_after_prelock(
                            &mut tx,
                            account_id,
                            application_id,
                            run_id,
                            fence,
                            &lease_token_sha256,
                            Some(*input),
                            authenticated_worker_id,
                            volume_worker_id,
                        )
                        .map_err(execution_lease_from_managed_cloud_error)?
                        .ok_or(ExecutionLeaseError::Conflict)?;
                    (Some(resolution.authority), resolution.db_time_ms)
                } else {
                    let now = original_source_db_now_postgres(&mut tx)
                        .map_err(execution_lease_from_original_source_error)?;
                    let current = resolve_current_execution_authority_postgres_after_prelock_at_ms(
                        &mut tx,
                        account_id,
                        &execution_target.application,
                        ExecutionAuthorityRunner::Cloud,
                        now,
                    )?;
                    if !current.authorized {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    (None, now)
                };
            let capacity = submission_evidence_capacity_at_ms(capacity, now);
            validate_submission_evidence_capacity_binding(
                account_id,
                application_id,
                run_id,
                &capacity,
                now,
            )?;
            if let Some((_, authenticated_worker_id, volume_worker_id)) = managed_worker.as_ref() {
                revalidate_managed_execution_volume_worker_postgres_tx(
                    &mut tx,
                    account_id,
                    run_id,
                    authenticated_worker_id,
                    volume_worker_id,
                    now,
                )?;
            } else {
                require_current_runner_volume_binding_postgres_for_operation(
                    &mut tx, account_id, run_id, now,
                )?;
            }
            if lease.phase == "click_started" {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase != "prepared" || lease.lease_expires_at_ms <= now {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.browser_profile_id != execution_target.browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            let ats_certified_receipt_authority = match ats_phase_b_context(
                account_id,
                application_id,
                run_id,
                final_submit_proof,
            )? {
                Some(request) => match
                    validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock(
                        &mut tx, &request, now,
                    )
                    .map_err(execution_lease_from_ats_certification_error)?
                {
                    AtsCertificationPhaseBTransactionOutcome::Authorized(result) => {
                        Some(result.ats_certified_receipt_authority)
                    }
                    AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined => {
                        tx.commit()?;
                        return Err(ExecutionLeaseError::Conflict);
                    }
                },
                None => None,
            };
            bind_final_submit_proof_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                final_submit_proof,
                now,
            )?;
            crate::db::object_uploads::reserve_submission_evidence_capacity_postgres_tx(
                &mut tx,
                &capacity,
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
            if let Some(authority) = managed_cloud.as_ref() {
                insert_managed_cloud_irreversible_effect_receipt_postgres_tx(
                    &mut tx,
                    account_id,
                    application_id,
                    run_id,
                    fence,
                    &lease_token_sha256,
                    authority,
                )
                .map_err(execution_lease_from_managed_cloud_error)?;
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(AuthorizedIrreversibleExecutionLeaseRecord {
                record: irreversible_execution_lease_record(
                    run_id,
                    fence,
                    lease_expires_at_ms,
                    ats_certified_receipt_authority,
                ),
                managed_cloud,
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

#[derive(Debug)]
struct LockedSubmissionEvidenceCapacity {
    runner: String,
    state: String,
    expires_at_ms: i64,
}

fn lock_submission_evidence_capacity_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<Option<LockedSubmissionEvidenceCapacity>> {
    Ok(tx
        .query_opt(
            "SELECT runner, state, expires_at_ms
               FROM jobs_submission_evidence_capacity
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
              FOR UPDATE",
            &[&account_id, &application_id, &run_id],
        )?
        .map(|row| LockedSubmissionEvidenceCapacity {
            runner: row.get(0),
            state: row.get(1),
            expires_at_ms: row.get(2),
        }))
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
    let caller_now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_identity_sqlite_for_operation(
                &tx, account_id, run_id, caller_now,
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
                caller_now,
            )?;
            tx.commit()?;
            Ok(authority)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            // This first call is a mutation-free prelock/early-denial pass. Its
            // process clock is not authoritative; every temporal predicate is
            // repeated below with a DB clock sampled after the exact rows lock.
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, caller_now,
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
            let capacity = lock_submission_evidence_capacity_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
            )?;
            let now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            let authority = validate_execution_receipt_authority(
                lease,
                finished_at_ms,
                capacity.as_ref().map(|value| value.runner.clone()),
                capacity.as_ref().map(|value| value.state.clone()),
                capacity.as_ref().map(|value| value.expires_at_ms),
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
    let caller_now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_identity_sqlite_for_operation(
                &tx, account_id, run_id, caller_now,
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
                        caller_now,
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
                    caller_now,
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
                caller_now,
                caller_now,
                matches!(outcome, "submitted" | "side_effect_unknown"),
            )?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            // Mutation-free prelock only. The caller clock cannot authorize a
            // terminal effect after a wait on the exact lease/capacity rows.
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, caller_now,
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
            let _capacity = lock_submission_evidence_capacity_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
            )?;
            let now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
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
            if matches!(outcome, "submitted" | "side_effect_unknown")
                && lease.lease_expires_at_ms <= now
            {
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
            // Mutation-free fleet/account/volume prelock. The process clock is
            // intentionally non-authoritative and is repeated after every exact
            // reconciliation row has been acquired below.
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
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
            let _capacity = lock_submission_evidence_capacity_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
            )?;
            let now = original_source_db_now_postgres(&mut tx)
                .map_err(execution_lease_from_original_source_error)?;
            require_current_runner_volume_identity_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            validate_checkpoint_lease_access(
                &lease,
                owner_id,
                lease_token,
                fence,
                checkpoint_version,
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
