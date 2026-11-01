// Active admission only. Historical heartbeat/finish/receipt replay must retain their
// existing lease/receipt authority rather than acquiring this mutable command dependency.

fn execution_session_material<'a>(
    command: &'a JobsWorkflowCommand,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    has_request_started: bool,
) -> ExecutionLeaseResult<(&'a str, &'a str, &'a str)> {
    if !matches!(
        command.state,
        JobsWorkflowCommandState::Delivering
            | JobsWorkflowCommandState::DeliveryUnknown
            | JobsWorkflowCommandState::Accepted
    ) {
        return Err(ExecutionLeaseError::Conflict);
    }
    historical_execution_session_material(
        command,
        account_id,
        application_id,
        run_id,
        has_request_started,
    )
}

// Immutable material validation only. Historical recovery deliberately ignores delivery state,
// current policy/head selection and admission time; it is not an active-admission resolver.
fn historical_execution_session_material<'a>(
    command: &'a JobsWorkflowCommand,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    has_request_started: bool,
) -> ExecutionLeaseResult<(&'a str, &'a str, &'a str)> {
    if command.account_id != account_id
        || command.application_id != application_id
        || command.run_id != run_id
        || command.protocol_version != WORKFLOW_COMMAND_PROTOCOL_VERSION
        || command.first_request_started_at_ms.is_none()
        || !has_request_started
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let (workflow, session_id, result_request_id) =
        match (&command.command_kind, &command.envelope.payload) {
            (JobsWorkflowCommandKind::Start, JobsWorkflowCommandPayload::Start(material)) => (
                &material.workflow_input,
                material.browser_session_id.as_str(),
                material.result_request_id.as_str(),
            ),
            (JobsWorkflowCommandKind::Resume, JobsWorkflowCommandPayload::Resume(material)) => (
                &material.workflow_input,
                material.browser_session_id.as_str(),
                material.result_request_id.as_str(),
            ),
            _ => return Err(ExecutionLeaseError::Conflict),
        };
    let identity_id = workflow
        .get("applicationIdentityId")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let profile_id = workflow
        .get("browserProfileId")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 160))
        .ok_or(ExecutionLeaseError::Conflict)?;
    if !validate_execution_binding(session_id, 128)
        || result_request_id != command.request_id
        || workflow.get("accountId").and_then(Value::as_str) != Some(account_id)
        || workflow.get("applicationId").and_then(Value::as_str) != Some(application_id)
        || workflow.get("idempotencyKey").and_then(Value::as_str) != Some(run_id)
        || workflow.get("runner").and_then(Value::as_str) != Some("cloud")
        || execution_browser_profile_id(account_id, identity_id) != profile_id
        || workflow
            .pointer("/packet/applicationId")
            .and_then(Value::as_str)
            != Some(application_id)
        || workflow
            .pointer("/packet/applicationIdentityId")
            .and_then(Value::as_str)
            != Some(identity_id)
        || workflow
            .pointer("/packet/browserProfileId")
            .and_then(Value::as_str)
            != Some(profile_id)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok((session_id, identity_id, profile_id))
}

fn execution_session_start(
    rows: Vec<(StoredWorkflowCommandRow, bool)>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<JobsWorkflowCommand> {
    require_single_execution_session_start(rows.len())?;
    let (stored, has_request_started) = rows
        .into_iter()
        .next()
        .ok_or(ExecutionLeaseError::NotFound)?;
    let command = workflow_command_from_stored(stored)?;
    if command.command_kind != JobsWorkflowCommandKind::Start {
        return Err(ExecutionLeaseError::Conflict);
    }
    execution_session_material(
        &command,
        account_id,
        application_id,
        run_id,
        has_request_started,
    )?;
    Ok(command)
}

fn require_single_execution_session_start(count: usize) -> ExecutionLeaseResult<()> {
    match count {
        0 => Err(ExecutionLeaseError::NotFound),
        1 => Ok(()),
        _ => Err(ExecutionLeaseError::Conflict),
    }
}

fn execution_session_frozen_application(
    start: &JobsWorkflowCommand,
    application: &JobApplication,
) -> ExecutionLeaseResult<()> {
    let (_, identity_id, profile_id) = execution_session_material(
        start,
        &start.account_id,
        &start.application_id,
        &start.run_id,
        true,
    )?;
    execution_session_application_binding(start, application, identity_id, profile_id)
}

fn execution_session_application_binding(
    start: &JobsWorkflowCommand,
    application: &JobApplication,
    identity_id: &str,
    profile_id: &str,
) -> ExecutionLeaseResult<()> {
    if application.id != start.application_id
        || application.run_id.as_deref() != Some(start.run_id.as_str())
        || application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            != Some(identity_id)
        || application
            .receipt
            .pointer("/approved_execution/packet/applicationId")
            .and_then(Value::as_str)
            != Some(start.application_id.as_str())
        || application
            .receipt
            .pointer("/approved_execution/packet/applicationIdentityId")
            .and_then(Value::as_str)
            != Some(identity_id)
        || application
            .receipt
            .pointer("/approved_execution/packet/browserProfileId")
            .and_then(Value::as_str)
            != Some(profile_id)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn execution_session_chain(
    start: &JobsWorkflowCommand,
    current: &JobsWorkflowCommand,
    request_command_id: &str,
    execution_command_id: &str,
    binding: &ManagedCloudWorkflowBindingInput,
    has_request_started: bool,
) -> ExecutionLeaseResult<()> {
    let start_material = execution_session_material(
        start,
        &start.account_id,
        &start.application_id,
        &start.run_id,
        true,
    )?;
    let current_material = execution_session_material(
        current,
        &start.account_id,
        &start.application_id,
        &start.run_id,
        has_request_started,
    )?;
    if start.command_kind != JobsWorkflowCommandKind::Start
        || request_command_id != current.id
        || execution_command_id != start.id
        || binding.command_id != current.id
        || binding.account_id != start.account_id
        || binding.application_id != start.application_id
        || binding.run_id != start.run_id
        || binding.workflow_id != start.workflow_id
        || current.workflow_id != start.workflow_id
        || start_material != current_material
        || (current.command_kind == JobsWorkflowCommandKind::Start && current.id != start.id)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn sqlite_managed_execution_browser_session_id(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
) -> ExecutionLeaseResult<String> {
    crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(tx, account_id)?;
    require_no_workflow_cleanup_sqlite_tx(tx, account_id)?;
    let authority = input
        .map(|input| {
            validate_managed_cloud_execution_lease_claim_input(input)?;
            sqlite_managed_cloud_execution_command_authority(
                tx,
                account_id,
                application_id,
                run_id,
                input,
            )
        })
        .transpose()
        .map_err(execution_lease_from_managed_cloud_error)?;
    let mut statement = tx.prepare(&format!(
        "SELECT {WORKFLOW_COMMAND_SELECT}, EXISTS(
           SELECT 1 FROM jobs_workflow_command_attempt_events event
            WHERE event.command_id=command.id AND event.account_id=command.account_id
              AND event.event_kind='request_started') AS execution_session_request_started
         FROM jobs_workflow_commands command
         WHERE command.account_id=?1 AND command.application_id=?2 AND command.run_id=?3
           AND command.command_kind='start'
         ORDER BY command.created_at_ms,command.id LIMIT 2"
    ))?;
    let rows = statement
        .query_map(params![account_id, application_id, run_id], |row| {
            Ok((
                sqlite_workflow_command_row(row)?,
                row.get::<_, bool>("execution_session_request_started")?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let start = execution_session_start(rows, account_id, application_id, run_id)?;
    if let (Some(input), Some(authority)) = (input, authority.as_ref()) {
        let (stored, has_request_started) = tx
            .query_row(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}, EXISTS(
               SELECT 1 FROM jobs_workflow_command_attempt_events event
                WHERE event.command_id=command.id AND event.account_id=command.account_id
                  AND event.event_kind='request_started') AS execution_session_request_started
             FROM jobs_workflow_commands command
             WHERE command.account_id=?1 AND command.application_id=?2 AND command.run_id=?3
               AND command.id=?4 AND command.request_id=?5"
                ),
                params![
                    account_id,
                    application_id,
                    run_id,
                    authority.request_command_id,
                    input.workflow_request_id
                ],
                |row| {
                    Ok((
                        sqlite_workflow_command_row(row)?,
                        row.get::<_, bool>("execution_session_request_started")?,
                    ))
                },
            )
            .optional()?
            .ok_or(ExecutionLeaseError::Conflict)?;
        let current = workflow_command_from_stored(stored)?;
        execution_session_chain(
            &start,
            &current,
            &authority.request_command_id,
            &authority.execution_binding.command_id,
            &authority.binding_input,
            has_request_started,
        )?;
    }
    // Caller already owns the active application/current-authority prelocks. This read adds
    // no new lock class and binds durable workflow material to that exact approved identity.
    let (job_id, raw): (String, String) = tx
        .query_row(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=?1 AND id=?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        raw,
        application_id,
        &job_id,
        "execution session application",
    )?;
    execution_session_frozen_application(&start, &application)?;
    let (session_id, _, _) =
        execution_session_material(&start, account_id, application_id, run_id, true)?;
    Ok(session_id.to_string())
}

fn postgres_managed_execution_browser_session_id(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    input: Option<&ManagedCloudExecutionLeaseClaimInput>,
) -> ExecutionLeaseResult<String> {
    // Caller already owns its global/current-authority/volume prelocks. Account comes before
    // command rows. With a managed request, preserve the existing request -> binding -> START
    // order before taking any additional active-material locks; never lock START first.
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(tx, account_id)?;
    require_no_workflow_cleanup_postgres_tx(tx, account_id)?;
    let authority = if let Some(input) = input {
        validate_managed_cloud_execution_lease_claim_input(input)
            .map_err(execution_lease_from_managed_cloud_error)?;
        Some(
            postgres_managed_cloud_execution_command_authority(
                tx,
                account_id,
                application_id,
                run_id,
                input,
                true,
            )
            .map_err(execution_lease_from_managed_cloud_error)?,
        )
    } else {
        None
    };
    let rows = tx.query(
        &format!(
            "SELECT {WORKFLOW_COMMAND_SELECT}, EXISTS(
           SELECT 1 FROM jobs_workflow_command_attempt_events event
            WHERE event.command_id=command.id AND event.account_id=command.account_id
              AND event.event_kind='request_started') AS execution_session_request_started
         FROM jobs_workflow_commands command
         WHERE command.account_id=$1 AND command.application_id=$2 AND command.run_id=$3
           AND command.command_kind='start'
         ORDER BY command.created_at_ms,command.id LIMIT 2 FOR SHARE OF command"
        ),
        &[&account_id, &application_id, &run_id],
    )?;
    let rows = rows
        .iter()
        .map(|row| {
            Ok((
                postgres_workflow_command_row(row),
                row.try_get::<_, bool>("execution_session_request_started")?,
            ))
        })
        .collect::<std::result::Result<Vec<_>, postgres::Error>>()?;
    let start = execution_session_start(rows, account_id, application_id, run_id)?;
    if let (Some(input), Some(authority)) = (input, authority.as_ref()) {
        let row = tx
            .query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}, EXISTS(
               SELECT 1 FROM jobs_workflow_command_attempt_events event
                WHERE event.command_id=command.id AND event.account_id=command.account_id
                  AND event.event_kind='request_started') AS execution_session_request_started
             FROM jobs_workflow_commands command
             WHERE command.account_id=$1 AND command.application_id=$2 AND command.run_id=$3
               AND command.id=$4 AND command.request_id=$5 FOR SHARE OF command"
                ),
                &[
                    &account_id,
                    &application_id,
                    &run_id,
                    &authority.request_command_id,
                    &input.workflow_request_id,
                ],
            )?
            .ok_or(ExecutionLeaseError::Conflict)?;
        let current = workflow_command_from_stored(postgres_workflow_command_row(&row))?;
        execution_session_chain(
            &start,
            &current,
            &authority.request_command_id,
            &authority.execution_binding.command_id,
            &authority.binding_input,
            row.try_get("execution_session_request_started")?,
        )?;
    }
    // Do not re-acquire an application lock after command locks. The outer active-admission
    // transaction already owns it; only read the frozen payload under those existing prelocks.
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let job_id: String = row.try_get(0)?;
    let application = parse_application_json(
        row.try_get(1)?,
        application_id,
        &job_id,
        "execution session application",
    )?;
    execution_session_frozen_application(&start, &application)?;
    let (session_id, _, _) =
        execution_session_material(&start, account_id, application_id, run_id, true)?;
    Ok(session_id.to_string())
}

#[derive(Clone)]
struct HistoricalExecutionSessionCommand {
    command: JobsWorkflowCommand,
    managed_required: bool,
    has_request_started_event: bool,
    binding: ManagedCloudStoredWorkflowBinding,
    request_start: ManagedCloudStoredRequestStartAuthority,
}

fn historical_execution_session_binding(
    history: &HistoricalExecutionSessionCommand,
    scope: &ManagedCloudScope,
) -> ExecutionLeaseResult<ManagedCloudWorkflowBinding> {
    require_exact_managed_cloud_workflow_binding(
        &history.binding,
        &ManagedCloudWorkflowBindingInput {
            command_id: history.command.id.clone(),
            account_id: history.command.account_id.clone(),
            application_id: history.command.application_id.clone(),
            run_id: history.command.run_id.clone(),
            workflow_id: history.command.workflow_id.clone(),
            scope: scope.clone(),
        },
    )
    .map_err(execution_lease_from_managed_cloud_error)
}

fn validate_historical_request_start(
    history: &HistoricalExecutionSessionCommand,
    binding: &ManagedCloudWorkflowBinding,
    execution: &ManagedCloudWorkflowBinding,
) -> ExecutionLeaseResult<()> {
    let stored = &history.request_start;
    if !history.managed_required
        || !history.has_request_started_event
        || history.command.first_request_started_at_ms.is_none()
        || stored.command_id != history.command.id
        || stored.account_id != history.command.account_id
        || stored.binding_sha256 != binding.binding_sha256
        || stored.fence <= 0
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let bytes = managed_cloud_decode_base64url(&stored.gateway_authority_base64url)
        .map_err(execution_lease_from_managed_cloud_error)?;
    if managed_cloud_sha256(&bytes) != stored.gateway_authority_sha256 {
        return Err(ExecutionLeaseError::Conflict);
    }
    let gateway: ManagedCloudGatewayAuthority =
        managed_cloud_parse_canonical(&bytes).map_err(execution_lease_from_managed_cloud_error)?;
    validate_managed_cloud_gateway_authority(&gateway, execution)
        .map_err(execution_lease_from_managed_cloud_error)?;
    if gateway.authorization.authorized_at_ms != stored.authorized_at_ms {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn historical_checkpoint_session(
    account_id: &str,
    application: &JobApplication,
    run_id: &str,
    lease: &StoredExecutionLease,
    stored: &ManagedCloudStoredExecutionLeaseAuthority,
    start: &HistoricalExecutionSessionCommand,
    request: &HistoricalExecutionSessionCommand,
) -> ExecutionLeaseResult<String> {
    let start_material = historical_execution_session_material(
        &start.command,
        account_id,
        &application.id,
        run_id,
        start.has_request_started_event,
    )?;
    let request_material = historical_execution_session_material(
        &request.command,
        account_id,
        &application.id,
        run_id,
        request.has_request_started_event,
    )?;
    if lease.account_id != account_id
        || lease.application_id != application.id
        || lease.browser_profile_id != start_material.2
        || start.command.command_kind != JobsWorkflowCommandKind::Start
        || start.command.workflow_id != request.command.workflow_id
        || start_material != request_material
        || stored.execution_command_id.as_deref() != Some(start.command.id.as_str())
        || stored.request_command_id.as_deref() != Some(request.command.id.as_str())
        || stored.workflow_request_id.as_deref() != Some(request.command.request_id.as_str())
        || (request.command.command_kind == JobsWorkflowCommandKind::Start
            && request.command.id != start.command.id)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    execution_session_application_binding(
        &start.command,
        application,
        start_material.1,
        start_material.2,
    )?;
    // RESUME may freeze newly reviewed answers. Its exact packet, not a mutable current session
    // or the older START packet's answer content, must match this lease's frozen application.
    let request_workflow = match &request.command.envelope.payload {
        JobsWorkflowCommandPayload::Start(material) => &material.workflow_input,
        JobsWorkflowCommandPayload::Resume(material) => &material.workflow_input,
    };
    let approved = approved_submission_snapshot(account_id, application)?;
    let packet = request_workflow
        .get("packet")
        .ok_or(ExecutionLeaseError::Conflict)?;
    let frozen = application
        .receipt
        .get("approved_execution")
        .ok_or(ExecutionLeaseError::Conflict)?;
    let schema = frozen
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or(ExecutionLeaseError::Conflict)?;
    if packet.get("approvedPacketChecksum").and_then(Value::as_str) != Some(approved.checksum)
        || packet
            .get("approvedExecutionSchemaVersion")
            .and_then(Value::as_i64)
            != Some(schema)
        || (matches!(schema, 2 | 3)
            && packet.get("approvedExecutionAdmission") != frozen.get("admission"))
        || (schema == 1 && packet.get("approvedExecutionAdmission").is_some())
        || request_workflow.get("job") != Some(approved.job)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    // Match the existing validate_stage_workflow_input transport normalization exactly.
    let mut comparable_packet = packet.clone();
    let fields = comparable_packet
        .as_object_mut()
        .ok_or(ExecutionLeaseError::Conflict)?;
    for field in [
        "approvedPacketChecksum",
        "approvedExecutionSchemaVersion",
        "approvedExecutionAdmission",
        "executionAuthority",
    ] {
        fields.remove(field);
    }
    if canonical_workflow_command_value(&comparable_packet)?
        != canonical_workflow_command_value(approved.packet)?
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let memo_bytes = managed_cloud_decode_base64url(
        stored
            .release_memo_base64url
            .as_deref()
            .ok_or(ExecutionLeaseError::Conflict)?,
    )
    .map_err(execution_lease_from_managed_cloud_error)?;
    let memo = parse_managed_cloud_release_memo_bytes(&memo_bytes)
        .map_err(execution_lease_from_managed_cloud_error)?;
    let execution_binding =
        historical_execution_session_binding(start, &memo.execution.admission.scope)?;
    let request_binding =
        historical_execution_session_binding(request, &memo.execution.admission.scope)?;
    if request_binding.admission != execution_binding.admission {
        return Err(ExecutionLeaseError::Conflict);
    }
    validate_historical_request_start(start, &execution_binding, &execution_binding)?;
    validate_historical_request_start(request, &request_binding, &execution_binding)?;
    let input = ManagedCloudExecutionLeaseClaimInput {
        workflow_request_id: request.command.request_id.clone(),
        managed_cloud_release: memo,
        managed_cloud_release_sha256: stored
            .release_sha256
            .clone()
            .ok_or(ExecutionLeaseError::Conflict)?,
        managed_cloud_runtime_instance_id: stored
            .runtime_instance_id
            .clone()
            .ok_or(ExecutionLeaseError::Conflict)?,
        managed_cloud_runtime_instance_epoch: stored
            .runtime_instance_epoch
            .ok_or(ExecutionLeaseError::Conflict)?,
    };
    validate_managed_cloud_execution_lease_claim_input(&input)
        .map_err(execution_lease_from_managed_cloud_error)?;
    // Existing pure canonical validation authenticates all twelve retained fields against this
    // exact lease generation/token and owner. It performs no current-head or expiry resolution.
    require_managed_cloud_stored_claim_allows_effect(
        stored,
        &ManagedCloudExecutionCommandAuthority {
            request_command_id: request.command.id.clone(),
            execution_binding,
            binding_input: ManagedCloudWorkflowBindingInput {
                command_id: request.command.id.clone(),
                account_id: account_id.to_string(),
                application_id: application.id.clone(),
                run_id: run_id.to_string(),
                workflow_id: request.command.workflow_id.clone(),
                scope: input
                    .managed_cloud_release
                    .execution
                    .admission
                    .scope
                    .clone(),
            },
        },
        &input,
        ManagedCloudStoredEffectContext {
            run_id,
            fence: lease.fence,
            lease_token_sha256: &lease.lease_token_sha256,
            authenticated_worker_id: &lease.owner_id,
            volume_worker_id: &lease.owner_id,
        },
    )
    .map_err(execution_lease_from_managed_cloud_error)?;
    Ok(start_material.0.to_string())
}

// No row/advisory locks: the caller's locked surviving lease pins exact command/binding FKs,
// while its account fence excludes authorized deletion. Final assembled validation below
// authenticates immutable payloads and request-start/event history across these MVCC reads.
fn sqlite_historical_session_command(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    id: &str,
) -> ExecutionLeaseResult<HistoricalExecutionSessionCommand> {
    let (row, managed_required, has_request_started_event) = tx.query_row(&format!(
        "SELECT {WORKFLOW_COMMAND_SELECT}, managed_cloud_authority_required=1,
         EXISTS(SELECT 1 FROM jobs_managed_cloud_request_start_authorities authority
          JOIN jobs_workflow_command_attempt_events event
           ON event.command_id=authority.command_id AND event.account_id=authority.account_id
            AND event.attempt_id=authority.attempt_id AND event.fence=authority.fence
          WHERE authority.command_id=command.id AND authority.account_id=command.account_id
            AND event.event_kind='request_started' AND event.event_phase='request_started')
         FROM jobs_workflow_commands command WHERE account_id=?1 AND application_id=?2 AND run_id=?3 AND id=?4"),
        params![account_id, application_id, run_id, id], |row| Ok((sqlite_workflow_command_row(row)?, row.get(28)?, row.get(29)?)))
        .optional()?.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(HistoricalExecutionSessionCommand {
        command: workflow_command_from_stored(row)?,
        managed_required,
        has_request_started_event,
        binding: sqlite_managed_cloud_workflow_binding(tx, id)
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?,
        request_start: sqlite_managed_cloud_stored_request_start_authority(tx, id)
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?,
    })
}

fn postgres_historical_session_command(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    id: &str,
) -> ExecutionLeaseResult<HistoricalExecutionSessionCommand> {
    let row = tx.query_opt(&format!(
        "SELECT {WORKFLOW_COMMAND_SELECT}, managed_cloud_authority_required IS TRUE,
         EXISTS(SELECT 1 FROM jobs_managed_cloud_request_start_authorities authority
          JOIN jobs_workflow_command_attempt_events event
           ON event.command_id=authority.command_id AND event.account_id=authority.account_id
            AND event.attempt_id=authority.attempt_id AND event.fence=authority.fence
          WHERE authority.command_id=command.id AND authority.account_id=command.account_id
            AND event.event_kind='request_started' AND event.event_phase='request_started')
         FROM jobs_workflow_commands command WHERE account_id=$1 AND application_id=$2 AND run_id=$3 AND id=$4"),
        &[&account_id, &application_id, &run_id, &id])?.ok_or(ExecutionLeaseError::Conflict)?;
    Ok(HistoricalExecutionSessionCommand {
        command: workflow_command_from_stored(postgres_workflow_command_row(&row))?,
        managed_required: row.try_get(28)?,
        has_request_started_event: row.try_get(29)?,
        binding: postgres_managed_cloud_workflow_binding(tx, id)
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?,
        request_start: postgres_managed_cloud_stored_request_start_authority(tx, id)
            .map_err(execution_lease_from_managed_cloud_error)?
            .ok_or(ExecutionLeaseError::Conflict)?,
    })
}

fn sqlite_checkpoint_browser_session_id(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    run_id: &str,
    lease: &StoredExecutionLease,
) -> ExecutionLeaseResult<String> {
    let stored = tx.query_row(
        &format!(
            "SELECT {MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_COLUMNS}
        FROM jobs_execution_leases WHERE account_id=?1 AND application_id=?2 AND run_id=?3"
        ),
        params![account_id, application.id, run_id],
        managed_cloud_execution_lease_authority_from_sqlite,
    )?;
    if !managed_cloud_execution_lease_authority_present(&stored)
        .map_err(execution_lease_from_managed_cloud_error)?
    {
        // Explicit historical compatibility only: managed markers OR bindings OR request-start
        // history must not be laundered into the legacy aligned-session contract.
        require_unmanaged_cloud_execution_sqlite_tx(tx, account_id, &application.id, run_id)
            .map_err(execution_lease_from_managed_cloud_error)?;
        return Ok(run_id.to_string());
    }
    let start = sqlite_historical_session_command(
        tx,
        account_id,
        &application.id,
        run_id,
        stored
            .execution_command_id
            .as_deref()
            .ok_or(ExecutionLeaseError::Conflict)?,
    )?;
    let request = sqlite_historical_session_command(
        tx,
        account_id,
        &application.id,
        run_id,
        stored
            .request_command_id
            .as_deref()
            .ok_or(ExecutionLeaseError::Conflict)?,
    )?;
    historical_checkpoint_session(
        account_id,
        application,
        run_id,
        lease,
        &stored,
        &start,
        &request,
    )
}

fn postgres_checkpoint_browser_session_id(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    run_id: &str,
    lease: &StoredExecutionLease,
) -> ExecutionLeaseResult<String> {
    let row = tx.query_one(
        &format!(
            "SELECT {MANAGED_CLOUD_EXECUTION_LEASE_AUTHORITY_COLUMNS}
        FROM jobs_execution_leases WHERE account_id=$1 AND application_id=$2 AND run_id=$3"
        ),
        &[&account_id, &application.id, &run_id],
    )?;
    let stored = managed_cloud_execution_lease_authority_from_postgres(&row);
    if !managed_cloud_execution_lease_authority_present(&stored)
        .map_err(execution_lease_from_managed_cloud_error)?
    {
        require_unmanaged_cloud_execution_postgres_tx(tx, account_id, &application.id, run_id)
            .map_err(execution_lease_from_managed_cloud_error)?;
        return Ok(run_id.to_string());
    }
    let start = postgres_historical_session_command(
        tx,
        account_id,
        &application.id,
        run_id,
        stored
            .execution_command_id
            .as_deref()
            .ok_or(ExecutionLeaseError::Conflict)?,
    )?;
    let request = postgres_historical_session_command(
        tx,
        account_id,
        &application.id,
        run_id,
        stored
            .request_command_id
            .as_deref()
            .ok_or(ExecutionLeaseError::Conflict)?,
    )?;
    historical_checkpoint_session(
        account_id,
        application,
        run_id,
        lease,
        &stored,
        &start,
        &request,
    )
}

#[cfg(test)]
mod execution_session_authority_tests {
    use super::*;

    fn start() -> JobsWorkflowCommand {
        let account_id = "session-authority-account";
        let application_id = "session-authority-application";
        let run_id = "session-authority-run";
        let request_id = "session-authority-request";
        let identity_id = "session-authority-identity";
        let profile_id = execution_browser_profile_id(account_id, identity_id);
        let workflow_input = json!({"accountId":account_id, "applicationId":application_id,
            "idempotencyKey":run_id, "runner":"cloud", "applicationIdentityId":identity_id,
            "browserProfileId":profile_id, "packet":{"applicationId":application_id,
                "applicationIdentityId":identity_id, "browserProfileId":profile_id}});
        let payload = JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
            workflow_input,
            browser_session_id: "cloud-session-authority-application".into(),
            result_request_id: request_id.into(),
        });
        JobsWorkflowCommand {
            id: "session-authority-command".into(),
            account_id: account_id.into(),
            application_id: application_id.into(),
            run_id: run_id.into(),
            workflow_id: "session-authority-workflow".into(),
            intervention_id: None,
            command_kind: JobsWorkflowCommandKind::Start,
            protocol_version: WORKFLOW_COMMAND_PROTOCOL_VERSION,
            idempotency_key_hmac_sha256: "a".repeat(64),
            request_id: request_id.into(),
            request_hmac_sha256: "b".repeat(64),
            payload_hmac_sha256: "c".repeat(64),
            state: JobsWorkflowCommandState::DeliveryUnknown,
            envelope: JobsWorkflowCommandEnvelope {
                schema_version: WORKFLOW_COMMAND_PROTOCOL_VERSION,
                command_kind: JobsWorkflowCommandKind::Start,
                request_id: request_id.into(),
                request_hmac_sha256: "b".repeat(64),
                payload_hmac_sha256: "c".repeat(64),
                workflow_id: "session-authority-workflow".into(),
                application_id: application_id.into(),
                run_id: run_id.into(),
                intervention_id: None,
                payload,
            },
            attempt_count: 1,
            fence: 1,
            lease_owner: None,
            lease_expires_at_ms: None,
            active_attempt_id: None,
            first_request_started_at_ms: Some(1),
            first_ambiguous_at_ms: Some(2),
            next_attempt_at_ms: None,
            last_outcome_code: None,
            temporal_run_id: None,
            accepted_at_ms: None,
            created_at_ms: 1,
            updated_at_ms: 2,
        }
    }

    type HistoricalFixture = (
        JobApplication,
        StoredExecutionLease,
        ManagedCloudStoredExecutionLeaseAuthority,
        HistoricalExecutionSessionCommand,
        HistoricalExecutionSessionCommand,
    );

    // Pure canonical retained-authority fixture, not a signed registry import or live claim.
    fn historical_fixture(auto: bool, resume: bool) -> HistoricalFixture {
        let mut command = start();
        command.request_id = "wfreq-v2-12345678-1234-5abc-8def-123456789abc".into();
        command.envelope.request_id = command.request_id.clone();
        let JobsWorkflowCommandPayload::Start(material) = &mut command.envelope.payload else {
            unreachable!()
        };
        // Keep all frozen request identifiers consistent before the first material validation.
        material.result_request_id = command.request_id.clone();
        command.first_request_started_at_ms = Some(1_000);
        command.state = JobsWorkflowCommandState::Cancelled;
        let (_, identity_id, profile_id) = historical_execution_session_material(
            &command,
            &command.account_id,
            &command.application_id,
            &command.run_id,
            true,
        )
        .unwrap();
        let identity_id = identity_id.to_string();
        let profile_id = profile_id.to_string();
        let packet = json!({"applicationId":command.application_id,"jobId":"historical-job",
            "resumeVersionId":"historical-resume","applicationIdentityId":identity_id,
            "applicationEmail":"synthetic@example.test","browserProfileId":profile_id,
            "answers":{},"verifiedClaimIds":[]});
        let job = json!({"canonicalUrl":"https://boards.greenhouse.io/synthetic/jobs/1"});
        let (schema, admission) = if auto {
            (
                3,
                json!({"kind":"track_auto_submit",
            "authorization_id":"historical-auto","career_track_id":"historical-track","revision_no":1,
            "authority_fingerprint":"a".repeat(64),"ats_certification":{
                "schema_version":1,"provider":"greenhouse","adapter_version":"2026.07.1-beta.1",
                "activation_generation":1,"activation_sha256":"a".repeat(64),"adapter_bundle_sha256":"b".repeat(64),
                "expires_at_ms":2_000,"layout_contract_version":1,"layout_set_sha256":"c".repeat(64),
                "manifest_sha256":"d".repeat(64),"runner_target_sha256s":["e".repeat(64)],
                "surface_sha256":"f".repeat(64),"target_key_sha256":"1".repeat(64),"variant_key":"greenhouse_public"}}),
            )
        } else {
            (2, json!({"kind":"review_approval"}))
        };
        let checksum =
            approved_submission_checksum(schema, &packet, &job, Some(&admission)).unwrap();
        let application: JobApplication =
            serde_json::from_value(json!({"id":command.application_id,
            "job_id":"historical-job","resume_version_id":"historical-resume","state":"queued",
            "submission_mode":if auto {"auto_submit"} else {"review_first"},"run_id":command.run_id,
            "receipt":{"application_identity":{"id":identity_id,"email":"synthetic@example.test"},
                "approved_execution":{"schema_version":schema,"packet":packet,"job":job,
                    "admission":admission,"checksum":checksum}}}))
            .unwrap();
        let mut transport = packet;
        transport["approvedPacketChecksum"] = json!(checksum);
        transport["approvedExecutionSchemaVersion"] = json!(schema);
        transport["approvedExecutionAdmission"] = admission;
        if let JobsWorkflowCommandPayload::Start(material) = &mut command.envelope.payload {
            material.workflow_input["packet"] = transport;
            material.workflow_input["job"] = job;
        }
        let mut current = command.clone();
        if resume {
            current.id = "session-authority-resume".into();
            current.command_kind = JobsWorkflowCommandKind::Resume;
            current.intervention_id = Some("historical-intervention".into());
            current.request_id = "wfreq-v2-12345678-1234-5abc-8def-123456789abd".into();
            current.envelope.command_kind = current.command_kind;
            current.envelope.intervention_id = current.intervention_id.clone();
            current.envelope.request_id = current.request_id.clone();
            let JobsWorkflowCommandPayload::Start(material) = &command.envelope.payload else {
                unreachable!()
            };
            current.envelope.payload =
                JobsWorkflowCommandPayload::Resume(JobsWorkflowResumeMaterial {
                    workflow_input: material.workflow_input.clone(),
                    browser_session_id: material.browser_session_id.clone(),
                    result_request_id: current.request_id.clone(),
                    resolution: json!({}),
                });
        }
        assert_eq!(current.envelope.request_id, current.request_id);
        historical_execution_session_material(
            &current,
            &current.account_id,
            &current.application_id,
            &current.run_id,
            true,
        )
        .expect("the START or RESUME fixture has consistent frozen request/material identifiers");
        let admission: ManagedCloudAdmissionAuthority = serde_json::from_value(json!({
            "scope":{"environment":"staging","region":"us-east-1","channel":"general"},
            "headRevision":1,"transitionSha256":"a".repeat(64),"activationSha256":"b".repeat(64),
            "manifestSha256":"c".repeat(64),"cohortSha256":"d".repeat(64),"trustGeneration":1,
            "channelSequence":1,"releaseId":"historical-release","releaseSequence":1,
            "taskQueueSha256":"e".repeat(64),"failureConverterSha256":"f".repeat(64),
            "readinessSha256":"1".repeat(64),"activationExpiresAtMs":2_000,"resolvedAtMs":1_000}))
        .unwrap();
        let make_binding = |command: &JobsWorkflowCommand| {
            new_managed_cloud_workflow_binding(
                &ManagedCloudWorkflowBindingInput {
                    command_id: command.id.clone(),
                    account_id: command.account_id.clone(),
                    application_id: command.application_id.clone(),
                    run_id: command.run_id.clone(),
                    workflow_id: command.workflow_id.clone(),
                    scope: admission.scope.clone(),
                },
                admission.clone(),
                1_000,
            )
            .unwrap()
        };
        let start_binding = make_binding(&command);
        let request_binding = make_binding(&current);
        let resolved = |binding: &ManagedCloudStoredWorkflowBinding| {
            managed_cloud_workflow_binding_result(
                binding.command_id.clone(),
                binding.binding_sha256.clone(),
                binding.release_memo_base64url.clone(),
                binding.release_memo_sha256.clone(),
                binding.admission.clone(),
                true,
            )
        };
        let execution = resolved(&start_binding);
        let history = |command: JobsWorkflowCommand, binding: ManagedCloudStoredWorkflowBinding| {
            let authority = managed_cloud_request_start_authority(
                resolved(&binding),
                execution.clone(),
                admission.clone(),
            )
            .unwrap();
            let bytes = managed_cloud_canonical_json(&authority.managed_cloud).unwrap();
            HistoricalExecutionSessionCommand {
                request_start: ManagedCloudStoredRequestStartAuthority {
                    attempt_id: "historical-attempt".into(),
                    account_id: command.account_id.clone(),
                    command_id: command.id.clone(),
                    fence: 1,
                    binding_sha256: binding.binding_sha256.clone(),
                    gateway_authority_base64url: encode_base64url(&bytes),
                    gateway_authority_sha256: managed_cloud_sha256(&bytes),
                    authorized_at_ms: 1_000,
                },
                command,
                binding,
                managed_required: true,
                has_request_started_event: true,
            }
        };
        let start = history(command, start_binding);
        let request = history(current, request_binding);
        let gateway = managed_cloud_request_start_authority(
            execution.clone(),
            execution.clone(),
            admission.clone(),
        )
        .unwrap()
        .managed_cloud;
        let gateway_bytes = managed_cloud_canonical_json(&gateway).unwrap();
        let lease = StoredExecutionLease {
            account_id: start.command.account_id.clone(),
            application_id: application.id.clone(),
            browser_profile_id: profile_id,
            owner_id: "historical-worker-1234567890".into(),
            lease_token_sha256: "a".repeat(64),
            fence: 2,
            phase: "prepared".into(),
            lease_expires_at_ms: 10,
        };
        let bound = bind_managed_cloud_execution_lease_authority(
            ManagedCloudExecutionLeaseAuthority {
                managed_cloud: gateway,
                managed_cloud_workflow_request_id: request.command.request_id.clone(),
                request_command_id: request.command.id.clone(),
                execution_command_id: start.command.id.clone(),
                binding_sha256: execution.binding_sha256.clone(),
                release_memo_base64url: execution.release_memo_base64url.clone(),
                release_sha256: execution.release_memo_sha256.clone(),
                managed_cloud_runtime_instance_id: "historical-runtime-instance".into(),
                managed_cloud_runtime_instance_epoch: 1,
                managed_cloud_worker_id: lease.owner_id.clone(),
                gateway_authority_base64url: encode_base64url(&gateway_bytes),
                gateway_authority_sha256: managed_cloud_sha256(&gateway_bytes),
            },
            &start.command.run_id,
            lease.fence,
            &lease.lease_token_sha256,
        )
        .unwrap();
        let stored = ManagedCloudStoredExecutionLeaseAuthority {
            workflow_request_id: Some(bound.authority.managed_cloud_workflow_request_id),
            request_command_id: Some(bound.authority.request_command_id),
            execution_command_id: Some(bound.authority.execution_command_id),
            binding_sha256: Some(bound.authority.binding_sha256),
            release_memo_base64url: Some(bound.authority.release_memo_base64url),
            release_sha256: Some(bound.authority.release_sha256),
            runtime_instance_id: Some(bound.authority.managed_cloud_runtime_instance_id),
            runtime_instance_epoch: Some(bound.authority.managed_cloud_runtime_instance_epoch),
            worker_id: Some(bound.authority.managed_cloud_worker_id),
            gateway_authority_base64url: Some(bound.authority.gateway_authority_base64url),
            gateway_authority_sha256: Some(bound.authority.gateway_authority_sha256),
            lease_authority_sha256: Some(bound.lease_authority_sha256),
        };
        (application, lease, stored, start, request)
    }

    #[test]
    fn historical_checkpoint_supports_both_modes_and_resume_without_active_state_or_time() {
        for auto in [false, true] {
            for resume in [false, true] {
                let (application, lease, stored, start, request) = historical_fixture(auto, resume);
                let session = historical_checkpoint_session(
                    &lease.account_id,
                    &application,
                    &start.command.run_id,
                    &lease,
                    &stored,
                    &start,
                    &request,
                )
                .unwrap();
                assert_eq!(session, "cloud-session-authority-application");
                assert_ne!(session, start.command.run_id);
                assert_eq!(start.command.state, JobsWorkflowCommandState::Cancelled);
                // No clock/head/policy input exists here: expiry cannot revoke historical recovery.
            }
        }
    }

    #[test]
    fn historical_checkpoint_rejects_metadata_packet_profile_run_and_event_drift() {
        for variant in 0..12 {
            let (mut application, mut lease, mut stored, start, mut request) =
                historical_fixture(false, true);
            match variant {
                0 => stored.release_sha256 = None,
                1 => stored.worker_id = Some("foreign-worker".into()),
                2 => stored.request_command_id = Some(start.command.id.clone()),
                3 => stored.lease_authority_sha256 = Some("f".repeat(64)),
                4 => lease.fence += 1,
                5 => lease.lease_token_sha256 = "b".repeat(64),
                6 => application.run_id = Some("newer-session-reuse-run".into()),
                7 => lease.browser_profile_id = "foreign-profile".into(),
                8 => request.has_request_started_event = false,
                9 => request.managed_required = false,
                10 => request.request_start.gateway_authority_sha256 = "a".repeat(64),
                11 => {
                    if let JobsWorkflowCommandPayload::Resume(material) =
                        &mut request.command.envelope.payload
                    {
                        material.workflow_input["packet"]["answers"] = json!({"changed":true});
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                historical_checkpoint_session(
                    &lease.account_id,
                    &application,
                    &start.command.run_id,
                    &lease,
                    &stored,
                    &start,
                    &request
                )
                .is_err(),
                "variant {variant}"
            );
        }
    }

    #[test]
    fn checkpoint_plan_and_exact_replay_use_retained_distinct_session_in_both_modes() {
        for auto in [false, true] {
            let (application, mut lease, _, start, _) = historical_fixture(auto, false);
            let session_id = "cloud-session-authority-application";
            let session = json!({"id":session_id,"runner":"cloud","status":"queued",
                "application_id":application.id,"current_step":"Waiting"});
            let session_raw = serde_json::to_string(&session).unwrap();
            let account_id = lease.account_id.clone();
            let plan = checkpoint_reconciliation_plan(
                &account_id,
                &application.id,
                &start.command.run_id,
                session_id,
                &lease,
                2,
                "prepared",
                &application.job_id,
                serde_json::to_string(&application).unwrap(),
                "queued",
                session_raw.clone(),
                "cloud",
                "queued",
                None,
                3_000,
            )
            .unwrap();
            assert_eq!(plan.browser_session.id, session_id);
            assert_eq!(plan.outcome, CheckpointRecoveryOutcome::Released);
            lease.phase = "released".into();
            assert_eq!(
                exact_checkpoint_recovery_replay(
                    &account_id,
                    &application.id,
                    &start.command.run_id,
                    session_id,
                    &lease,
                    2,
                    "prepared",
                    &application.job_id,
                    &serde_json::to_string(&plan.application).unwrap(),
                    "failed",
                    &serde_json::to_string(&plan.browser_session).unwrap(),
                    "cloud",
                    "failed",
                    None
                )
                .unwrap(),
                Some(CheckpointRecoveryOutcome::Released)
            );
            let mut reused_application = application.clone();
            reused_application.run_id = Some("newer-run-using-the-same-session".into());
            assert!(checkpoint_reconciliation_plan(
                &account_id,
                &application.id,
                &start.command.run_id,
                session_id,
                &lease,
                2,
                "prepared",
                &application.job_id,
                serde_json::to_string(&reused_application).unwrap(),
                "queued",
                session_raw,
                "cloud",
                "queued",
                None,
                3_000
            )
            .is_err());
            assert_eq!(session["status"], "queued");
        }
    }

    #[test]
    fn historical_lease_metadata_never_falls_back_when_partial() {
        let (_, _, mut stored, _, _) = historical_fixture(false, false);
        assert!(managed_cloud_execution_lease_authority_present(&stored).unwrap());
        stored.workflow_request_id = None;
        assert!(managed_cloud_execution_lease_authority_present(&stored).is_err());
    }

    #[test]
    fn checkpoint_postgres_submitted_return_precedes_history_and_session_dependency() {
        let source = include_str!("execution_leases.rs");
        let reconciliation = source
            .split("pub fn reconcile_execution_lease_checkpoint(")
            .nth(1)
            .unwrap();
        for branch in ["DbPool::Sqlite(_) =>", "DbPool::Postgres(_) =>"] {
            let body = reconciliation.split(branch).nth(1).unwrap();
            let submitted = body
                .find("if validate_trusted_submitted_checkpoint(")
                .unwrap();
            assert!(body.find("validate_checkpoint_lease_access(").unwrap() < submitted);
            assert!(submitted < body.find("_checkpoint_browser_session_id(").unwrap());
            assert!(
                submitted
                    < body
                        .find("SELECT session_json, runner, status FROM jobs_browser_sessions")
                        .unwrap()
            );
        }
        let postgres = reconciliation
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .unwrap();
        let capacity = postgres
            .find("lock_submission_evidence_capacity_postgres_tx(")
            .unwrap();
        let final_time = postgres
            .rfind("let now = original_source_db_now_postgres")
            .unwrap();
        assert!(capacity < final_time);
        assert!(
            final_time
                < postgres
                    .find("let plan = checkpoint_reconciliation_plan(")
                    .unwrap()
        );
    }

    fn resolve(
        command: &JobsWorkflowCommand,
        started: bool,
    ) -> ExecutionLeaseResult<(&str, &str, &str)> {
        execution_session_material(
            command,
            "session-authority-account",
            "session-authority-application",
            "session-authority-run",
            started,
        )
    }

    #[test]
    fn durable_material_accepts_distinct_session_id_in_all_active_states() {
        for state in [
            JobsWorkflowCommandState::Delivering,
            JobsWorkflowCommandState::DeliveryUnknown,
            JobsWorkflowCommandState::Accepted,
        ] {
            let mut command = start();
            command.state = state;
            let (session_id, _, _) = resolve(&command, true).unwrap();
            assert_eq!(session_id, "cloud-session-authority-application");
            assert_ne!(session_id, command.run_id);
        }
    }

    #[test]
    fn durable_material_denies_inactive_or_unstarted_commands() {
        for state in [
            JobsWorkflowCommandState::Pending,
            JobsWorkflowCommandState::Claimed,
            JobsWorkflowCommandState::IdentityConflict,
            JobsWorkflowCommandState::Rejected,
            JobsWorkflowCommandState::Cancelled,
        ] {
            let mut command = start();
            command.state = state;
            assert!(matches!(
                resolve(&command, true),
                Err(ExecutionLeaseError::Conflict)
            ));
        }
        let mut command = start();
        assert!(resolve(&command, false).is_err());
        command.first_request_started_at_ms = None;
        assert!(resolve(&command, true).is_err());
    }

    #[test]
    fn durable_material_denies_cross_scope_and_frozen_identity_profile_drift() {
        for pointer in [
            "/accountId",
            "/applicationId",
            "/idempotencyKey",
            "/runner",
            "/applicationIdentityId",
            "/browserProfileId",
            "/packet/applicationId",
            "/packet/applicationIdentityId",
            "/packet/browserProfileId",
        ] {
            let mut command = start();
            let JobsWorkflowCommandPayload::Start(material) = &mut command.envelope.payload else {
                unreachable!()
            };
            *material.workflow_input.pointer_mut(pointer).unwrap() = json!("foreign");
            assert!(
                matches!(resolve(&command, true), Err(ExecutionLeaseError::Conflict)),
                "{pointer}"
            );
        }
        for mutate in [0, 1, 2, 3] {
            let mut command = start();
            match mutate {
                0 => command.account_id = "foreign".into(),
                1 => command.application_id = "foreign".into(),
                2 => command.run_id = "foreign".into(),
                _ => {
                    let JobsWorkflowCommandPayload::Start(material) = &mut command.envelope.payload
                    else {
                        unreachable!()
                    };
                    material.browser_session_id.clear();
                }
            }
            assert!(resolve(&command, true).is_err());
        }
    }

    #[test]
    fn durable_start_requires_exactly_one_row() {
        assert!(matches!(
            require_single_execution_session_start(0),
            Err(ExecutionLeaseError::NotFound)
        ));
        assert!(require_single_execution_session_start(1).is_ok());
        assert!(matches!(
            require_single_execution_session_start(2),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert!(matches!(
            require_single_execution_session_start(3),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    fn binding(current: &JobsWorkflowCommand) -> ManagedCloudWorkflowBindingInput {
        ManagedCloudWorkflowBindingInput {
            command_id: current.id.clone(),
            account_id: current.account_id.clone(),
            application_id: current.application_id.clone(),
            run_id: current.run_id.clone(),
            workflow_id: current.workflow_id.clone(),
            scope: ManagedCloudScope {
                environment: "staging".into(),
                region: "us-east-1".into(),
                channel: "general".into(),
            },
        }
    }

    fn resume(start: &JobsWorkflowCommand) -> JobsWorkflowCommand {
        let mut command = start.clone();
        command.id = "session-authority-resume-command".into();
        command.request_id = "session-authority-resume-request".into();
        command.envelope.request_id = command.request_id.clone();
        command.intervention_id = Some("session-authority-intervention".into());
        command.envelope.intervention_id = command.intervention_id.clone();
        command.command_kind = JobsWorkflowCommandKind::Resume;
        command.envelope.command_kind = command.command_kind;
        let JobsWorkflowCommandPayload::Start(material) = &start.envelope.payload else {
            unreachable!()
        };
        command.envelope.payload = JobsWorkflowCommandPayload::Resume(JobsWorkflowResumeMaterial {
            workflow_input: material.workflow_input.clone(),
            browser_session_id: material.browser_session_id.clone(),
            result_request_id: command.request_id.clone(),
            resolution: json!({"action":"retry"}),
        });
        command
    }

    #[test]
    fn managed_chain_accepts_only_exact_start_or_resume_to_same_start() {
        let start = start();
        let resume = resume(&start);
        for current in [&start, &resume] {
            assert!(execution_session_chain(
                &start,
                current,
                &current.id,
                &start.id,
                &binding(current),
                true
            )
            .is_ok());
            assert!(execution_session_chain(
                &start,
                current,
                "foreign-request-command",
                &start.id,
                &binding(current),
                true
            )
            .is_err());
            assert!(execution_session_chain(
                &start,
                current,
                &current.id,
                "foreign-start-command",
                &binding(current),
                true
            )
            .is_err());
            assert!(execution_session_chain(
                &start,
                current,
                &current.id,
                &start.id,
                &binding(current),
                false
            )
            .is_err());
        }
        for field in [0, 1, 2, 3, 4] {
            let mut changed = binding(&resume);
            match field {
                0 => changed.command_id = "foreign".into(),
                1 => changed.account_id = "foreign".into(),
                2 => changed.application_id = "foreign".into(),
                3 => changed.run_id = "foreign".into(),
                _ => changed.workflow_id = "foreign".into(),
            }
            assert!(execution_session_chain(
                &start, &resume, &resume.id, &start.id, &changed, true
            )
            .is_err());
        }
    }

    #[test]
    fn managed_resume_cannot_switch_workflow_session_identity_or_profile() {
        let start = start();
        for field in [0, 1, 2] {
            let mut changed = resume(&start);
            match field {
                0 => changed.workflow_id = "foreign-workflow".into(),
                1 => {
                    let JobsWorkflowCommandPayload::Resume(material) =
                        &mut changed.envelope.payload
                    else {
                        unreachable!()
                    };
                    material.browser_session_id = "foreign-session".into();
                }
                _ => {
                    let JobsWorkflowCommandPayload::Resume(material) =
                        &mut changed.envelope.payload
                    else {
                        unreachable!()
                    };
                    let identity = "other-valid-identity";
                    let profile = execution_browser_profile_id(&start.account_id, identity);
                    material.workflow_input["applicationIdentityId"] = json!(identity);
                    material.workflow_input["browserProfileId"] = json!(profile);
                    material.workflow_input["packet"]["applicationIdentityId"] = json!(identity);
                    material.workflow_input["packet"]["browserProfileId"] = json!(profile);
                }
            }
            assert!(execution_session_chain(
                &start,
                &changed,
                &changed.id,
                &start.id,
                &binding(&changed),
                true
            )
            .is_err());
        }
    }

    fn application(start: &JobsWorkflowCommand) -> JobApplication {
        let (_, identity_id, profile_id) = resolve(start, true).unwrap();
        serde_json::from_value(json!({"id":start.application_id, "job_id":"session-job",
            "state":"queued", "run_id":start.run_id, "receipt":{
                "application_identity":{"id":identity_id}, "approved_execution":{"packet":{
                    "applicationId":start.application_id, "applicationIdentityId":identity_id,
                    "browserProfileId":profile_id}}}}))
        .unwrap()
    }

    #[test]
    fn frozen_application_requires_exact_durable_identity_profile_application_and_run() {
        let start = start();
        let expected = application(&start);
        assert!(execution_session_frozen_application(&start, &expected).is_ok());
        for pointer in [
            "/application_identity/id",
            "/approved_execution/packet/applicationId",
            "/approved_execution/packet/applicationIdentityId",
            "/approved_execution/packet/browserProfileId",
        ] {
            let mut changed = expected.clone();
            *changed.receipt.pointer_mut(pointer).unwrap() = json!("foreign");
            assert!(
                execution_session_frozen_application(&start, &changed).is_err(),
                "{pointer}"
            );
        }
        let mut changed = expected.clone();
        changed.id = "foreign-application".into();
        assert!(execution_session_frozen_application(&start, &changed).is_err());
        let mut changed = expected.clone();
        changed.run_id = Some("foreign-run".into());
        assert!(execution_session_frozen_application(&start, &changed).is_err());
        let mut changed = expected;
        changed.run_id = None;
        assert!(execution_session_frozen_application(&start, &changed).is_err());
    }

    #[test]
    fn frozen_application_does_not_fallback_when_approved_packet_binding_is_missing() {
        let start = start();
        for key in ["applicationId", "applicationIdentityId", "browserProfileId"] {
            let mut changed = application(&start);
            changed.receipt["approved_execution"]["packet"]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                execution_session_frozen_application(&start, &changed).is_err(),
                "{key}"
            );
        }
    }
}
