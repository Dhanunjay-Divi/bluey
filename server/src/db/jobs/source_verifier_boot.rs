// Source-verifier boot generation authority.
//
// Phase 628 bound original-source-verifier startup identity to
// `(runtime_instance_id, instance_epoch)`. A lost ready response therefore
// forced a restarted lifecycle to collide with the stored startup row, and a
// late `probe_failed` heartbeat could degrade an already-ready runtime.
//
// This module owns the naturally idempotent replacement:
//
// 1. the server assigns a strictly monotonic boot generation (clients never
//    propose one);
// 2. registration and startup are idempotent by request id, replaying the
//    exact stored canonical response bytes;
// 3. a reused request id with a different canonical payload is an explicit
//    conflict; and
// 4. readiness is fenced by generation, so a stale boot can never lower a
//    newer committed ready generation.
//
// Tables are created by SQLite migration
// `063_jobs_source_verifier_boot_generation_authority.sql` and PostgreSQL
// migration `041_jobs_source_verifier_boot_generation_authority.sql`.

/// Canonical registration request. The boot generation is deliberately absent:
/// it is server-assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceVerifierBootRegistrationRequest {
    pub session_token: String,
    pub registration_request_id: String,
    pub boot_id: String,
    pub runtime_instance_id: String,
    pub instance_epoch: i64,
    pub worker_id: String,
    pub runtime_identity_sha256: String,
    pub activation_sha256: String,
    pub manifest_sha256: String,
    pub component_id: String,
    pub role: String,
}

/// Result of an idempotent registration. `replayed` is a server-side detail and
/// is not part of the canonical response bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceVerifierBootRegistration {
    pub boot_id: String,
    pub boot_generation: i64,
    pub canonical_response_base64url: String,
    pub response_sha256: String,
    pub registered_at_ms: i64,
    pub replayed: bool,
}

/// Canonical startup request bound to an already registered boot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceVerifierBootStartupRequest {
    pub session_token: String,
    pub startup_request_id: String,
    pub boot_id: String,
    pub runtime_instance_id: String,
    pub instance_epoch: i64,
    pub boot_generation: i64,
    pub worker_id: String,
    pub degraded_heartbeat_sequence: i64,
}

/// Result of an idempotent startup transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceVerifierBootStartup {
    pub startup_request_id: String,
    pub boot_generation: i64,
    pub degraded_heartbeat_sequence: i64,
    pub ready_heartbeat_sequence: i64,
    pub probe_result: String,
    pub canonical_response_base64url: String,
    pub response_sha256: String,
    pub startup_at_ms: i64,
    pub replayed: bool,
}

fn source_verifier_boot_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Deterministic, length-prefixed field encoding. Length prefixes prevent
/// boundary ambiguity between adjacent fields.
fn source_verifier_boot_canonical_bytes(fields: &BTreeMap<&str, String>) -> Vec<u8> {
    let mut out = Vec::new();
    for (key, value) in fields {
        out.extend_from_slice(&(key.len() as u64).to_be_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(value.len() as u64).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }
    out
}

fn source_verifier_boot_canonical_json(fields: &BTreeMap<&str, String>) -> Vec<u8> {
    let ordered: BTreeMap<&str, &str> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    serde_json::to_vec(&ordered).unwrap_or_default()
}

fn source_verifier_boot_registration_fields(
    request: &SourceVerifierBootRegistrationRequest,
) -> BTreeMap<&'static str, String> {
    let mut fields: BTreeMap<&'static str, String> = BTreeMap::new();
    fields.insert("activationSha256", request.activation_sha256.clone());
    fields.insert("bootId", request.boot_id.clone());
    fields.insert("componentId", request.component_id.clone());
    fields.insert("instanceEpoch", request.instance_epoch.to_string());
    fields.insert("manifestSha256", request.manifest_sha256.clone());
    fields.insert(
        "registrationRequestId",
        request.registration_request_id.clone(),
    );
    fields.insert("role", request.role.clone());
    fields.insert(
        "runtimeIdentitySha256",
        request.runtime_identity_sha256.clone(),
    );
    fields.insert("runtimeInstanceId", request.runtime_instance_id.clone());
    fields.insert("workerId", request.worker_id.clone());
    fields
}

pub(crate) fn source_verifier_boot_registration_request_sha256(
    request: &SourceVerifierBootRegistrationRequest,
) -> String {
    let mut fields = source_verifier_boot_registration_fields(request);
    fields.insert(
        "sessionTokenSha256",
        source_verifier_boot_sha256_hex(request.session_token.as_bytes()),
    );
    source_verifier_boot_sha256_hex(&source_verifier_boot_canonical_bytes(&fields))
}

fn source_verifier_boot_startup_fields(
    request: &SourceVerifierBootStartupRequest,
) -> BTreeMap<&'static str, String> {
    let mut fields: BTreeMap<&'static str, String> = BTreeMap::new();
    fields.insert("bootGeneration", request.boot_generation.to_string());
    fields.insert("bootId", request.boot_id.clone());
    fields.insert(
        "degradedHeartbeatSequence",
        request.degraded_heartbeat_sequence.to_string(),
    );
    fields.insert("instanceEpoch", request.instance_epoch.to_string());
    fields.insert("runtimeInstanceId", request.runtime_instance_id.clone());
    fields.insert("startupRequestId", request.startup_request_id.clone());
    fields.insert("workerId", request.worker_id.clone());
    fields
}

pub(crate) fn source_verifier_boot_startup_request_sha256(
    request: &SourceVerifierBootStartupRequest,
) -> String {
    let mut fields = source_verifier_boot_startup_fields(request);
    fields.insert(
        "sessionTokenSha256",
        source_verifier_boot_sha256_hex(request.session_token.as_bytes()),
    );
    source_verifier_boot_sha256_hex(&source_verifier_boot_canonical_bytes(&fields))
}

fn validate_source_verifier_boot_identity(
    instance: &ManagedCloudRuntimeInstance,
    session_proof: &str,
    runtime_instance_id: &str,
    instance_epoch: i64,
    worker_id: &str,
    session_token: &str,
) -> Result<()> {
    if instance.runtime_instance_id != runtime_instance_id
        || instance.instance_epoch != instance_epoch
        || instance.worker_id != worker_id
        || instance.component_id != "jobs-workflows"
        || instance.role != "original_source_verifier"
        || !managed_cloud_safe_integer(instance_epoch, true)
    {
        anyhow::bail!("source-verifier boot runtime identity conflicts")
    }
    let expected = managed_cloud_runtime_session_proof_hmac(
        session_token,
        &instance.grant_id,
        worker_id,
        runtime_instance_id,
        instance_epoch,
    )?;
    require_managed_cloud_secret_match(session_proof, &expected)?;
    Ok(())
}

fn source_verifier_boot_instance_sqlite(
    tx: &rusqlite::Transaction<'_>,
    runtime_instance_id: &str,
    instance_epoch: i64,
    worker_id: &str,
    session_token: &str,
) -> Result<ManagedCloudRuntimeInstance> {
    let (instance, proof) = sqlite_managed_cloud_runtime_instance_by_runtime_id(
        tx,
        runtime_instance_id,
        MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
    )?
    .ok_or_else(|| anyhow::anyhow!("source-verifier boot runtime not found"))?;
    validate_source_verifier_boot_identity(
        &instance,
        &proof,
        runtime_instance_id,
        instance_epoch,
        worker_id,
        session_token,
    )?;
    let now = managed_cloud_db_now_sqlite(tx)?;
    require_sqlite_managed_cloud_runtime_authority_active(tx, &instance, now)?;
    let admitted: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols
          WHERE manifest_sha256=?1 AND protocol_id='source_verifier_boot'
            AND protocol_version=1 AND ordinal=11)",
        params![instance.manifest_sha256],
        |row| row.get(0),
    )?;
    if !admitted {
        anyhow::bail!("source-verifier boot protocol is not admitted")
    }
    Ok(instance)
}

fn source_verifier_boot_instance_postgres(
    tx: &mut postgres::Transaction<'_>,
    runtime_instance_id: &str,
    instance_epoch: i64,
    worker_id: &str,
    session_token: &str,
) -> Result<ManagedCloudRuntimeInstance> {
    // Match the real lease path: H -> M -> runtime/boot -> D -> assignment.
    lock_operational_hold_shared_postgres_tx(tx)?;
    lock_managed_cloud_release_registry_exclusive_postgres_tx(tx)?;
    let row = tx
        .query_opt(
            &format!(
                "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS} FROM {}
          WHERE runtime_instance_id=$1 AND instance_epoch=$2 FOR UPDATE",
                MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES.instances
            ),
            &[&runtime_instance_id, &instance_epoch],
        )?
        .ok_or_else(|| anyhow::anyhow!("source-verifier boot runtime not found"))?;
    let (instance, proof) = managed_cloud_runtime_instance_from_postgres(&row);
    validate_source_verifier_boot_identity(
        &instance,
        &proof,
        runtime_instance_id,
        instance_epoch,
        worker_id,
        session_token,
    )?;
    let now = managed_cloud_db_now_postgres(tx)?;
    require_postgres_managed_cloud_runtime_authority_active(tx, &instance, now)?;
    let admitted: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols
          WHERE manifest_sha256=$1 AND protocol_id='source_verifier_boot'
            AND protocol_version=1 AND ordinal=11)",
            &[&instance.manifest_sha256],
        )?
        .get(0);
    if !admitted {
        anyhow::bail!("source-verifier boot protocol is not admitted")
    }
    Ok(instance)
}

fn require_source_verifier_boot_registration_identity(
    instance: &ManagedCloudRuntimeInstance,
    request: &SourceVerifierBootRegistrationRequest,
) -> Result<()> {
    if !managed_cloud_token(&request.boot_id, 128)
        || request.boot_id.len() < 20
        || !managed_cloud_token(&request.registration_request_id, 128)
        || request.registration_request_id.len() < 20
        || instance.runtime_identity_sha256 != request.runtime_identity_sha256
        || instance.activation_sha256 != request.activation_sha256
        || instance.manifest_sha256 != request.manifest_sha256
        || instance.component_id != request.component_id
        || instance.role != request.role
    {
        anyhow::bail!("source-verifier boot registration identity conflicts")
    }
    Ok(())
}

/// Canonical registration response. Contains no mutable replay flag so that an
/// exact replay returns byte-identical output.
fn source_verifier_boot_registration_response(
    request: &SourceVerifierBootRegistrationRequest,
    boot_generation: i64,
    registered_at_ms: i64,
) -> (String, String, String) {
    let mut fields = source_verifier_boot_registration_fields(request);
    fields.insert("bootGeneration", boot_generation.to_string());
    fields.insert("registeredAtMs", registered_at_ms.to_string());
    fields.insert("protocolId", "source_verifier_boot".to_string());
    fields.insert("protocolVersion", "1".to_string());
    let canonical = source_verifier_boot_canonical_json(&fields);
    let response_sha256 = source_verifier_boot_sha256_hex(&canonical);
    let canonical_response_base64url =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical);
    let authority_sha256 = source_verifier_boot_sha256_hex(
        format!(
            "registration-authority\u{1f}{}\u{1f}{}\u{1f}{}",
            request.runtime_instance_id, request.instance_epoch, boot_generation
        )
        .as_bytes(),
    );
    (
        canonical_response_base64url,
        response_sha256,
        authority_sha256,
    )
}

/// Canonical startup response. Also free of any mutable replay flag.
fn source_verifier_boot_startup_response(
    request: &SourceVerifierBootStartupRequest,
    probe_result: &str,
    startup_at_ms: i64,
) -> (String, String, String) {
    let mut fields = source_verifier_boot_startup_fields(request);
    fields.insert("probeResult", probe_result.to_string());
    fields.insert(
        "readyHeartbeatSequence",
        (request.degraded_heartbeat_sequence + 1).to_string(),
    );
    fields.insert("startupAtMs", startup_at_ms.to_string());
    fields.insert("protocolId", "source_verifier_boot".to_string());
    fields.insert("protocolVersion", "1".to_string());
    let canonical = source_verifier_boot_canonical_json(&fields);
    let response_sha256 = source_verifier_boot_sha256_hex(&canonical);
    let canonical_response_base64url =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical);
    let authority_sha256 = source_verifier_boot_sha256_hex(
        format!(
            "startup-authority\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            request.runtime_instance_id,
            request.instance_epoch,
            request.boot_generation,
            request.degraded_heartbeat_sequence
        )
        .as_bytes(),
    );
    (
        canonical_response_base64url,
        response_sha256,
        authority_sha256,
    )
}

/// Assigns the next boot generation for an instance epoch. Monotonic: the
/// migration trigger rejects any attempt to lower it.
fn assign_source_verifier_boot_generation_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    runtime_instance_id: &str,
    instance_epoch: i64,
    now: i64,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO jobs_managed_cloud_source_verifier_boot_generation_heads (
            runtime_instance_id, instance_epoch, current_boot_generation,
            ready_boot_generation, updated_at_ms
         ) VALUES (?1, ?2, 1, NULL, ?3)
         ON CONFLICT(runtime_instance_id, instance_epoch) DO UPDATE SET
            current_boot_generation = current_boot_generation + 1,
            updated_at_ms = ?3",
        params![runtime_instance_id, instance_epoch, now],
    )?;
    let generation: i64 = tx.query_row(
        "SELECT current_boot_generation
           FROM jobs_managed_cloud_source_verifier_boot_generation_heads
          WHERE runtime_instance_id = ?1 AND instance_epoch = ?2",
        params![runtime_instance_id, instance_epoch],
        |row| row.get(0),
    )?;
    Ok(generation)
}

/// Idempotently registers a boot and returns its server-assigned generation.
///
/// An identical `registration_request_id` with the same canonical payload
/// replays the stored bytes. The same id with a different payload conflicts.
pub(crate) fn register_source_verifier_boot(
    pool: &DbPool,
    request: &SourceVerifierBootRegistrationRequest,
) -> Result<SourceVerifierBootRegistration> {
    let request_sha256 = source_verifier_boot_registration_request_sha256(request);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let instance = source_verifier_boot_instance_sqlite(
                &tx,
                &request.runtime_instance_id,
                request.instance_epoch,
                &request.worker_id,
                &request.session_token,
            )?;
            require_source_verifier_boot_registration_identity(&instance, request)?;
            let now = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT boot_id, boot_generation, registration_request_sha256,
                            canonical_response_base64url, response_sha256, registered_at_ms
                       FROM jobs_managed_cloud_source_verifier_boot_registrations
                      WHERE registration_request_id = ?1",
                    params![request.registration_request_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((boot_id, generation, stored_sha256, response, response_sha256, at_ms)) =
                existing
            {
                if stored_sha256 != request_sha256 {
                    anyhow::bail!("source-verifier boot registration request id conflicts")
                }
                tx.commit()?;
                return Ok(SourceVerifierBootRegistration {
                    boot_id,
                    boot_generation: generation,
                    canonical_response_base64url: response,
                    response_sha256,
                    registered_at_ms: at_ms,
                    replayed: true,
                });
            }
            let boot_generation = assign_source_verifier_boot_generation_sqlite_tx(
                &tx,
                &request.runtime_instance_id,
                request.instance_epoch,
                now,
            )?;
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_registration_response(request, boot_generation, now);
            tx.execute(
                "INSERT INTO jobs_managed_cloud_source_verifier_boot_registrations (
                    boot_id, registration_request_id, runtime_instance_id, instance_epoch,
                    boot_generation, worker_id, runtime_identity_sha256, activation_sha256,
                    manifest_sha256, component_id, role, registration_request_sha256,
                    registration_authority_sha256, canonical_response_base64url,
                    response_sha256, registered_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    request.boot_id,
                    request.registration_request_id,
                    request.runtime_instance_id,
                    request.instance_epoch,
                    boot_generation,
                    request.worker_id,
                    request.runtime_identity_sha256,
                    request.activation_sha256,
                    request.manifest_sha256,
                    request.component_id,
                    request.role,
                    request_sha256,
                    authority_sha256,
                    canonical_response_base64url,
                    response_sha256,
                    now,
                ],
            )?;
            tx.commit()?;
            Ok(SourceVerifierBootRegistration {
                boot_id: request.boot_id.clone(),
                boot_generation,
                canonical_response_base64url,
                response_sha256,
                registered_at_ms: now,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let instance = source_verifier_boot_instance_postgres(
                &mut tx,
                &request.runtime_instance_id,
                request.instance_epoch,
                &request.worker_id,
                &request.session_token,
            )?;
            require_source_verifier_boot_registration_identity(&instance, request)?;
            let now = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx.query_opt(
                "SELECT boot_id, boot_generation, registration_request_sha256,
                        canonical_response_base64url, response_sha256, registered_at_ms
                   FROM jobs_managed_cloud_source_verifier_boot_registrations
                  WHERE registration_request_id = $1",
                &[&request.registration_request_id],
            )?;
            if let Some(row) = existing {
                let stored_sha256: String = row.get(2);
                if stored_sha256 != request_sha256 {
                    anyhow::bail!("source-verifier boot registration request id conflicts")
                }
                let registration = SourceVerifierBootRegistration {
                    boot_id: row.get(0),
                    boot_generation: row.get(1),
                    canonical_response_base64url: row.get(3),
                    response_sha256: row.get(4),
                    registered_at_ms: row.get(5),
                    replayed: true,
                };
                tx.commit()?;
                return Ok(registration);
            }
            // Lock order is preserved: the generation head is taken after the
            // managed release registry and before discovery/assignment work.
            tx.execute(
                "INSERT INTO jobs_managed_cloud_source_verifier_boot_generation_heads (
                    runtime_instance_id, instance_epoch, current_boot_generation,
                    ready_boot_generation, updated_at_ms
                 ) VALUES ($1, $2, 1, NULL, $3)
                 ON CONFLICT(runtime_instance_id, instance_epoch) DO UPDATE SET
                    current_boot_generation =
                        jobs_managed_cloud_source_verifier_boot_generation_heads
                            .current_boot_generation + 1,
                    updated_at_ms = $3",
                &[&request.runtime_instance_id, &request.instance_epoch, &now],
            )?;
            let boot_generation: i64 = tx
                .query_one(
                    "SELECT current_boot_generation
                       FROM jobs_managed_cloud_source_verifier_boot_generation_heads
                      WHERE runtime_instance_id = $1 AND instance_epoch = $2
                        FOR UPDATE",
                    &[&request.runtime_instance_id, &request.instance_epoch],
                )?
                .get(0);
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_registration_response(request, boot_generation, now);
            tx.execute(
                "INSERT INTO jobs_managed_cloud_source_verifier_boot_registrations (
                    boot_id, registration_request_id, runtime_instance_id, instance_epoch,
                    boot_generation, worker_id, runtime_identity_sha256, activation_sha256,
                    manifest_sha256, component_id, role, registration_request_sha256,
                    registration_authority_sha256, canonical_response_base64url,
                    response_sha256, registered_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
                &[
                    &request.boot_id,
                    &request.registration_request_id,
                    &request.runtime_instance_id,
                    &request.instance_epoch,
                    &boot_generation,
                    &request.worker_id,
                    &request.runtime_identity_sha256,
                    &request.activation_sha256,
                    &request.manifest_sha256,
                    &request.component_id,
                    &request.role,
                    &request_sha256,
                    &authority_sha256,
                    &canonical_response_base64url,
                    &response_sha256,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok(SourceVerifierBootRegistration {
                boot_id: request.boot_id.clone(),
                boot_generation,
                canonical_response_base64url,
                response_sha256,
                registered_at_ms: now,
                replayed: false,
            })
        }
    })
}

fn require_source_verifier_boot_head_sqlite(
    tx: &rusqlite::Transaction<'_>,
    request: &SourceVerifierBootStartupRequest,
) -> Result<()> {
    let matched: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
           JOIN jobs_managed_cloud_source_verifier_boot_registrations registration
             ON registration.runtime_instance_id=head.runtime_instance_id
            AND registration.instance_epoch=head.instance_epoch
            AND registration.boot_generation=head.current_boot_generation
          WHERE head.runtime_instance_id=?1 AND head.instance_epoch=?2
            AND head.current_boot_generation=?3 AND registration.boot_id=?4
            AND registration.worker_id=?5)",
        params![
            request.runtime_instance_id,
            request.instance_epoch,
            request.boot_generation,
            request.boot_id,
            request.worker_id
        ],
        |row| row.get(0),
    )?;
    if !matched {
        anyhow::bail!("source-verifier boot startup generation conflicts")
    }
    Ok(())
}

fn require_source_verifier_boot_head_postgres(
    tx: &mut postgres::Transaction<'_>,
    request: &SourceVerifierBootStartupRequest,
) -> Result<()> {
    let matched = tx
        .query_opt(
            "SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_generation_heads head
           JOIN jobs_managed_cloud_source_verifier_boot_registrations registration
             ON registration.runtime_instance_id=head.runtime_instance_id
            AND registration.instance_epoch=head.instance_epoch
            AND registration.boot_generation=head.current_boot_generation
          WHERE head.runtime_instance_id=$1 AND head.instance_epoch=$2
            AND head.current_boot_generation=$3 AND registration.boot_id=$4
            AND registration.worker_id=$5 FOR UPDATE OF head",
            &[
                &request.runtime_instance_id,
                &request.instance_epoch,
                &request.boot_generation,
                &request.boot_id,
                &request.worker_id,
            ],
        )?
        .is_some();
    if !matched {
        anyhow::bail!("source-verifier boot startup generation conflicts")
    }
    Ok(())
}

fn source_verifier_boot_probe_binding(
    instance: &ManagedCloudRuntimeInstance,
    request: &SourceVerifierBootStartupRequest,
) -> Result<OriginalSourceVerifierBinding> {
    Ok(OriginalSourceVerifierBinding {
        worker_id: request.worker_id.clone(),
        runtime_instance_id: request.runtime_instance_id.clone(),
        runtime_instance_epoch: request.instance_epoch,
        runtime_authority_sha256: original_source_verifier_runtime_authority_sha256(instance)?,
        runtime_session_token: request.session_token.clone(),
        boot_id: Some(request.boot_id.clone()),
        boot_generation: Some(request.boot_generation),
    })
}

fn probe_source_verifier_boot_sqlite(
    tx: &mut rusqlite::Transaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    request: &SourceVerifierBootStartupRequest,
    now: i64,
) -> Result<String> {
    let binding = source_verifier_boot_probe_binding(instance, request)?;
    let (canonical, response_sha256, authority_sha256) =
        source_verifier_boot_startup_response(request, "idle", now);
    let request_sha256 = source_verifier_boot_startup_request_sha256(request);
    tx.execute_batch("SAVEPOINT bluey_osv_boot_probe")?;
    let probe = (|| -> Result<bool> {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities (
                startup_request_id,boot_id,runtime_instance_id,instance_epoch,boot_generation,
                worker_id,request_sha256,degraded_heartbeat_sequence,ready_heartbeat_sequence,
                probe_result,startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8+1,'idle',?9,?10,?11,?12)",
            params![request.startup_request_id,request.boot_id,request.runtime_instance_id,
                request.instance_epoch,request.boot_generation,request.worker_id,request_sha256,
                request.degraded_heartbeat_sequence,authority_sha256,canonical,response_sha256,now],
        )?;
        tx.execute(
            "UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads
                SET ready_boot_generation=?3,updated_at_ms=?4
              WHERE runtime_instance_id=?1 AND instance_epoch=?2 AND current_boot_generation=?3",
            params![
                request.runtime_instance_id,
                request.instance_epoch,
                request.boot_generation,
                now
            ],
        )?;
        commit_source_verifier_boot_ready_sqlite_tx(tx, request, now)?;
        Ok(probe_original_source_verification_leaseability_sqlite_tx(
            tx, &binding, now,
        )?)
    })();
    // Roll back even when probe/transition fails. No assignment, heartbeat,
    // event, boot authority or generation mutation survives this savepoint.
    let rollback = tx.execute_batch(
        "ROLLBACK TO SAVEPOINT bluey_osv_boot_probe; RELEASE SAVEPOINT bluey_osv_boot_probe",
    );
    let leaseable = probe?;
    rollback?;
    Ok(if leaseable { "leaseable" } else { "idle" }.to_string())
}

fn probe_source_verifier_boot_postgres(
    tx: &mut postgres::Transaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    request: &SourceVerifierBootStartupRequest,
    now: i64,
) -> Result<String> {
    let binding = source_verifier_boot_probe_binding(instance, request)?;
    let (canonical, response_sha256, authority_sha256) =
        source_verifier_boot_startup_response(request, "idle", now);
    let request_sha256 = source_verifier_boot_startup_request_sha256(request);
    let ready = request.degraded_heartbeat_sequence + 1;
    tx.batch_execute("SAVEPOINT bluey_osv_boot_probe")?;
    let probe = (|| -> Result<bool> {
        tx.execute(
            "INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities (
                startup_request_id,boot_id,runtime_instance_id,instance_epoch,boot_generation,
                worker_id,request_sha256,degraded_heartbeat_sequence,ready_heartbeat_sequence,
                probe_result,startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'idle',$10,$11,$12,$13)",
            &[&request.startup_request_id,&request.boot_id,&request.runtime_instance_id,
                &request.instance_epoch,&request.boot_generation,&request.worker_id,&request_sha256,
                &request.degraded_heartbeat_sequence,&ready,&authority_sha256,&canonical,&response_sha256,&now],
        )?;
        tx.execute(
            "UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads
                SET ready_boot_generation=$3,updated_at_ms=$4
              WHERE runtime_instance_id=$1 AND instance_epoch=$2 AND current_boot_generation=$3",
            &[
                &request.runtime_instance_id,
                &request.instance_epoch,
                &request.boot_generation,
                &now,
            ],
        )?;
        commit_source_verifier_boot_ready_postgres_tx(tx, request, now)?;
        Ok(probe_original_source_verification_leaseability_postgres_tx(
            tx, &binding, now,
        )?)
    })();
    let rollback = tx.batch_execute(
        "ROLLBACK TO SAVEPOINT bluey_osv_boot_probe; RELEASE SAVEPOINT bluey_osv_boot_probe",
    );
    let leaseable = probe?;
    rollback?;
    Ok(if leaseable { "leaseable" } else { "idle" }.to_string())
}

/// Idempotently records the one-shot startup readiness transition
/// `N degraded -> N+1 ready` for an exact boot generation.
///
/// A repeated `startup_request_id` with the same canonical payload replays the
/// stored bytes without re-running the probe. A different payload conflicts. A
/// stale generation is rejected by the migration fence.
///  MUST be determined by the server from the rollback-only
/// readiness probe. It is deliberately not part of the client request, so a
/// client can never dictate its own readiness.
pub(crate) fn record_source_verifier_boot_startup(
    pool: &DbPool,
    request: &SourceVerifierBootStartupRequest,
) -> Result<SourceVerifierBootStartup> {
    if !managed_cloud_safe_integer(request.boot_generation, true)
        || !(1..9007199254740991).contains(&request.degraded_heartbeat_sequence)
        || !managed_cloud_token(&request.startup_request_id, 128)
        || request.startup_request_id.len() < 20
    {
        anyhow::bail!("source-verifier boot startup request is invalid")
    }
    let request_sha256 = source_verifier_boot_startup_request_sha256(request);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let instance = source_verifier_boot_instance_sqlite(
                &tx,
                &request.runtime_instance_id,
                request.instance_epoch,
                &request.worker_id,
                &request.session_token,
            )?;
            let now = managed_cloud_db_now_sqlite(&tx)?;
            let existing = tx
                .query_row(
                    "SELECT request_sha256, boot_generation, degraded_heartbeat_sequence,
                            ready_heartbeat_sequence, probe_result,
                            canonical_response_base64url, response_sha256, startup_at_ms
                       FROM jobs_managed_cloud_source_verifier_boot_startup_authorities
                      WHERE startup_request_id = ?1",
                    params![request.startup_request_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((
                stored_sha256,
                boot_generation,
                degraded,
                ready,
                probe_result,
                response,
                response_sha256,
                at_ms,
            )) = existing
            {
                if stored_sha256 != request_sha256 {
                    anyhow::bail!("source-verifier boot startup request id conflicts")
                }
                tx.commit()?;
                return Ok(SourceVerifierBootStartup {
                    startup_request_id: request.startup_request_id.clone(),
                    boot_generation,
                    degraded_heartbeat_sequence: degraded,
                    ready_heartbeat_sequence: ready,
                    probe_result,
                    canonical_response_base64url: response,
                    response_sha256,
                    startup_at_ms: at_ms,
                    replayed: true,
                });
            }
            // Stored responses are immutable read-only results, not fresh execution
            // authority. A replacement boot must fence new work, not erase an
            // authenticated exact retry of a previously committed result.
            require_source_verifier_boot_head_sqlite(&tx, request)?;
            let probe_result = probe_source_verifier_boot_sqlite(&mut tx, &instance, request, now)?;
            let probe_result = probe_result.as_str();
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_startup_response(request, probe_result, now);
            tx.execute(
                "INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities (
                    startup_request_id, boot_id, runtime_instance_id, instance_epoch,
                    boot_generation, worker_id, request_sha256, degraded_heartbeat_sequence,
                    ready_heartbeat_sequence, probe_result, startup_authority_sha256,
                    canonical_response_base64url, response_sha256, startup_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    request.startup_request_id,
                    request.boot_id,
                    request.runtime_instance_id,
                    request.instance_epoch,
                    request.boot_generation,
                    request.worker_id,
                    request_sha256,
                    request.degraded_heartbeat_sequence,
                    request.degraded_heartbeat_sequence + 1,
                    probe_result,
                    authority_sha256,
                    canonical_response_base64url,
                    response_sha256,
                    now,
                ],
            )?;
            // Readiness may only advance. The migration trigger rejects a lower
            // ready generation, so a stale boot can never win.
            tx.execute(
                "UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads
                    SET ready_boot_generation = ?3, updated_at_ms = ?4
                  WHERE runtime_instance_id = ?1 AND instance_epoch = ?2
                    AND (ready_boot_generation IS NULL OR ready_boot_generation < ?3)",
                params![
                    request.runtime_instance_id,
                    request.instance_epoch,
                    request.boot_generation,
                    now,
                ],
            )?;
            commit_source_verifier_boot_ready_sqlite_tx(&tx, request, now)?;
            tx.commit()?;
            Ok(SourceVerifierBootStartup {
                startup_request_id: request.startup_request_id.clone(),
                boot_generation: request.boot_generation,
                degraded_heartbeat_sequence: request.degraded_heartbeat_sequence,
                ready_heartbeat_sequence: request.degraded_heartbeat_sequence + 1,
                probe_result: probe_result.to_string(),
                canonical_response_base64url,
                response_sha256,
                startup_at_ms: now,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let instance = source_verifier_boot_instance_postgres(
                &mut tx,
                &request.runtime_instance_id,
                request.instance_epoch,
                &request.worker_id,
                &request.session_token,
            )?;
            let now = managed_cloud_db_now_postgres(&mut tx)?;
            let existing = tx.query_opt(
                "SELECT request_sha256, boot_generation, degraded_heartbeat_sequence,
                        ready_heartbeat_sequence, probe_result,
                        canonical_response_base64url, response_sha256, startup_at_ms
                   FROM jobs_managed_cloud_source_verifier_boot_startup_authorities
                  WHERE startup_request_id = $1",
                &[&request.startup_request_id],
            )?;
            if let Some(row) = existing {
                let stored_sha256: String = row.get(0);
                if stored_sha256 != request_sha256 {
                    anyhow::bail!("source-verifier boot startup request id conflicts")
                }
                let startup = SourceVerifierBootStartup {
                    startup_request_id: request.startup_request_id.clone(),
                    boot_generation: row.get(1),
                    degraded_heartbeat_sequence: row.get(2),
                    ready_heartbeat_sequence: row.get(3),
                    probe_result: row.get(4),
                    canonical_response_base64url: row.get(5),
                    response_sha256: row.get(6),
                    startup_at_ms: row.get(7),
                    replayed: true,
                };
                tx.commit()?;
                return Ok(startup);
            }
            require_source_verifier_boot_head_postgres(&mut tx, request)?;
            let probe_result =
                probe_source_verifier_boot_postgres(&mut tx, &instance, request, now)?;
            let probe_result = probe_result.as_str();
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_startup_response(request, probe_result, now);
            let ready_sequence = request.degraded_heartbeat_sequence + 1;
            tx.execute(
                "INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities (
                    startup_request_id, boot_id, runtime_instance_id, instance_epoch,
                    boot_generation, worker_id, request_sha256, degraded_heartbeat_sequence,
                    ready_heartbeat_sequence, probe_result, startup_authority_sha256,
                    canonical_response_base64url, response_sha256, startup_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
                &[
                    &request.startup_request_id,
                    &request.boot_id,
                    &request.runtime_instance_id,
                    &request.instance_epoch,
                    &request.boot_generation,
                    &request.worker_id,
                    &request_sha256,
                    &request.degraded_heartbeat_sequence,
                    &ready_sequence,
                    &probe_result,
                    &authority_sha256,
                    &canonical_response_base64url,
                    &response_sha256,
                    &now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads
                    SET ready_boot_generation = $3, updated_at_ms = $4
                  WHERE runtime_instance_id = $1 AND instance_epoch = $2
                    AND (ready_boot_generation IS NULL OR ready_boot_generation < $3)",
                &[
                    &request.runtime_instance_id,
                    &request.instance_epoch,
                    &request.boot_generation,
                    &now,
                ],
            )?;
            commit_source_verifier_boot_ready_postgres_tx(&mut tx, request, now)?;
            tx.commit()?;
            Ok(SourceVerifierBootStartup {
                startup_request_id: request.startup_request_id.clone(),
                boot_generation: request.boot_generation,
                degraded_heartbeat_sequence: request.degraded_heartbeat_sequence,
                ready_heartbeat_sequence: ready_sequence,
                probe_result: probe_result.to_string(),
                canonical_response_base64url,
                response_sha256,
                startup_at_ms: now,
                replayed: false,
            })
        }
    })
}

#[cfg(test)]
mod source_verifier_boot_tests {
    use super::*;

    fn snapshot(pool: &DbPool, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
        let connection = pool.get().expect("snapshot connection");
        let mut statement = connection
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1,2"))
            .expect("snapshot prepared fixture table");
        let rows = statement
            .query_map([], |row| {
                (0..row.as_ref().column_count())
                    .map(|column| row.get(column))
                    .collect()
            })
            .expect("snapshot rows");
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .expect("snapshot values")
    }

    #[test]
    fn source_verifier_boot_migration_failure_and_replay_preserve_ready_authority_and_manifest_bytes() {
        let fixture = original_source_verifier_startup_test_fixture();
        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-replay-00000000001",
                "boot-replay-000000001",
            ),
        )
        .expect("register replay fixture");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &registration.boot_id,
            registration.boot_generation,
        );
        let request = db_startup(
            &fixture.request,
            "startup-replay-000000001",
            &registration.boot_id,
            registration.boot_generation,
        );
        let ready = record_source_verifier_boot_startup(&fixture.pool, &request)
            .expect("establish ready authority before migration replay");
        let tables = [
            "jobs_managed_cloud_manifests",
            "jobs_managed_cloud_manifest_components",
            "jobs_managed_cloud_source_verifier_boot_generation_heads",
            "jobs_managed_cloud_source_verifier_boot_registrations",
            "jobs_managed_cloud_source_verifier_boot_startup_authorities",
            "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings",
            "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
        ];
        let before: Vec<_> = tables
            .iter()
            .map(|table| snapshot(&fixture.pool, table))
            .collect();
        let schema_before = snapshot(&fixture.pool, "sqlite_master");
        for (migration, version) in [
            (crate::db::SQLITE_JOBS_SOURCE_VERIFIER_BOOT_GENERATION_AUTHORITY, "063"),
            (crate::db::SQLITE_JOBS_SOURCE_VERIFIER_HEARTBEAT_BOOT_GENERATION_FENCE, "064"),
            (crate::db::SQLITE_JOBS_SOURCE_VERIFIER_HEARTBEAT_BOOT_BINDINGS, "065"),
        ] {
            assert_eq!(migration.matches("COMMIT;").count(), 1);
            let interrupted = migration.replacen(
                "COMMIT;",
                "SELECT * FROM phase628_forced_missing_relation; COMMIT;",
                1,
            );
            let mut connection = fixture.pool.get().expect("failure injection connection");
            crate::db::execute_sqlite_source_verifier_boot_migration(
                &mut connection,
                &interrupted,
                version,
            )
            .expect_err("real migration must propagate the injected pre-commit failure");
            assert!(connection.is_autocommit());
            let enabled: bool = connection
                .pragma_query_value(None, "foreign_keys", |row| row.get(0))
                .expect("foreign-key mode after failed real migration");
            assert!(enabled);
            drop(connection);
            assert_eq!(schema_before, snapshot(&fixture.pool, "sqlite_master"));
            for (index, table) in tables.iter().enumerate() {
                assert_eq!(before[index], snapshot(&fixture.pool, table), "failure changed {table}");
            }
        }
        for _ in 0..2 {
            crate::db::run_migrations(&fixture.pool).expect("replay full SQLite migration chain");
            for (index, table) in tables.iter().enumerate() {
                assert_eq!(
                    before[index],
                    snapshot(&fixture.pool, table),
                    "replay changed {table}"
                );
            }
            let connection = fixture.pool.get().expect("check replay foreign keys");
            let violations: i64 = connection
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .expect("check replay foreign key consistency");
            assert_eq!(violations, 0);
        }
        let replay = record_source_verifier_boot_startup(&fixture.pool, &request)
            .expect("replay authority after migration replay");
        assert_eq!(
            ready.canonical_response_base64url,
            replay.canonical_response_base64url
        );
        assert_eq!(ready.startup_at_ms, replay.startup_at_ms);
    }

    fn registration_request() -> SourceVerifierBootRegistrationRequest {
        SourceVerifierBootRegistrationRequest {
            session_token: "unit-test-session-token".to_string(),
            registration_request_id: "reg-00000000000000001".to_string(),
            boot_id: "boot-0000000000000001".to_string(),
            runtime_instance_id: "ri-1".to_string(),
            instance_epoch: 1,
            worker_id: "w-1".to_string(),
            runtime_identity_sha256: "a".repeat(64),
            activation_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            component_id: "jobs-workflows".to_string(),
            role: "original_source_verifier".to_string(),
        }
    }

    fn startup_request() -> SourceVerifierBootStartupRequest {
        SourceVerifierBootStartupRequest {
            session_token: "unit-test-session-token".to_string(),
            startup_request_id: "sreq-000000000000001".to_string(),
            boot_id: "boot-0000000000000001".to_string(),
            runtime_instance_id: "ri-1".to_string(),
            instance_epoch: 1,
            boot_generation: 1,
            worker_id: "w-1".to_string(),
            degraded_heartbeat_sequence: 1,
        }
    }

    #[test]
    fn registration_digest_is_stable_and_payload_sensitive() {
        let request = registration_request();
        let first = source_verifier_boot_registration_request_sha256(&request);
        let second = source_verifier_boot_registration_request_sha256(&request);
        assert_eq!(first, second, "identical payloads must digest identically");
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        let mut changed = registration_request();
        changed.session_token = "different-unit-test-session-token".to_string();
        assert_ne!(
            first,
            source_verifier_boot_registration_request_sha256(&changed),
            "a changed payload must produce a conflicting digest"
        );
    }

    #[test]
    fn startup_digest_is_stable_and_payload_sensitive() {
        let request = startup_request();
        let first = source_verifier_boot_startup_request_sha256(&request);
        assert_eq!(
            first,
            source_verifier_boot_startup_request_sha256(&request.clone())
        );
        let mut changed = startup_request();
        changed.session_token = "different-unit-test-session-token".to_string();
        assert_ne!(first, source_verifier_boot_startup_request_sha256(&changed));
    }

    #[test]
    fn canonical_bytes_are_length_prefixed_and_unambiguous() {
        let mut left: BTreeMap<&str, String> = BTreeMap::new();
        left.insert("a", "bc".to_string());
        let mut right: BTreeMap<&str, String> = BTreeMap::new();
        right.insert("ab", "c".to_string());
        assert_ne!(
            source_verifier_boot_canonical_bytes(&left),
            source_verifier_boot_canonical_bytes(&right),
            "field boundaries must not be ambiguous"
        );
    }

    #[test]
    fn registration_response_excludes_replay_flag_and_is_deterministic() {
        let request = registration_request();
        let (first_response, first_sha, first_authority) =
            source_verifier_boot_registration_response(&request, 2, 1_000);
        let (second_response, second_sha, second_authority) =
            source_verifier_boot_registration_response(&request, 2, 1_000);
        assert_eq!(first_response, second_response);
        assert_eq!(first_sha, second_sha);
        assert_eq!(first_authority, second_authority);
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(first_response.as_bytes())
            .expect("canonical response must be base64url");
        let decoded = String::from_utf8_lossy(&decoded);
        assert!(decoded.contains("bootGeneration"));
        assert!(decoded.contains("source_verifier_boot"));
        assert!(
            !decoded.contains("replayed"),
            "canonical bytes must not carry a mutable replay flag"
        );
    }

    #[test]
    fn startup_response_binds_ready_sequence_to_degraded_plus_one() {
        let request = startup_request();
        let (response, _, _) = source_verifier_boot_startup_response(&request, "idle", 2_000);
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(response.as_bytes())
            .expect("canonical response must be base64url");
        let decoded = String::from_utf8_lossy(&decoded);
        assert!(decoded.contains("readyHeartbeatSequence"));
        assert!(decoded.contains("degradedHeartbeatSequence"));
        let (other, _, _) = source_verifier_boot_startup_response(&request, "idle", 3_000);
        assert_ne!(
            response, other,
            "startup time participates in canonical bytes"
        );
    }

    #[test]
    fn different_generations_produce_different_registration_authority() {
        let request = registration_request();
        let (_, _, generation_one) = source_verifier_boot_registration_response(&request, 1, 1_000);
        let (_, _, generation_two) = source_verifier_boot_registration_response(&request, 2, 1_000);
        assert_ne!(generation_one, generation_two);
    }

    // ---- DB-backed regression coverage (Phase 628 required cases) ----
    // Use the managed v3 release fixture, never the generic fake `test_pool`.
    use super::managed_cloud_release_authority_tests::original_source_verifier_startup_test_fixture;

    fn db_registration(
        fixture: &ManagedCloudOriginalSourceVerifierStartupRequest,
        request_id: &str,
        boot_id: &str,
    ) -> SourceVerifierBootRegistrationRequest {
        SourceVerifierBootRegistrationRequest {
            session_token: fixture.session_token.clone(),
            registration_request_id: request_id.to_string(),
            boot_id: boot_id.to_string(),
            runtime_instance_id: fixture.runtime_instance_id.clone(),
            instance_epoch: fixture.instance_epoch,
            worker_id: fixture.worker_id.clone(),
            runtime_identity_sha256: fixture.runtime_identity_sha256.clone(),
            activation_sha256: fixture.activation_sha256.clone(),
            manifest_sha256: fixture.manifest_sha256.clone(),
            component_id: fixture.component_id.clone(),
            role: fixture.role.clone(),
        }
    }

    fn db_startup(
        fixture: &ManagedCloudOriginalSourceVerifierStartupRequest,
        request_id: &str,
        boot_id: &str,
        boot_generation: i64,
    ) -> SourceVerifierBootStartupRequest {
        SourceVerifierBootStartupRequest {
            session_token: fixture.session_token.clone(),
            startup_request_id: request_id.to_string(),
            boot_id: boot_id.to_string(),
            runtime_instance_id: fixture.runtime_instance_id.clone(),
            instance_epoch: fixture.instance_epoch,
            boot_generation,
            worker_id: fixture.worker_id.clone(),
            degraded_heartbeat_sequence: 2,
        }
    }

    fn boot_heartbeat(
        fixture: &ManagedCloudOriginalSourceVerifierStartupRequest,
        sequence: i64,
        health_state: &str,
        reason_code: Option<&str>,
        boot_id: &str,
        boot_generation: i64,
    ) -> ManagedCloudRuntimeHeartbeatInput {
        ManagedCloudRuntimeHeartbeatInput {
            runtime_instance_id: fixture.runtime_instance_id.clone(),
            worker_id: fixture.worker_id.clone(),
            session_token: fixture.session_token.clone(),
            heartbeat_sequence: sequence,
            observed_head_revision: fixture.observed_head_revision,
            observed_transition_sha256: fixture.observed_transition_sha256.clone(),
            activation_sha256: fixture.activation_sha256.clone(),
            manifest_sha256: fixture.manifest_sha256.clone(),
            component_id: fixture.component_id.clone(),
            role: fixture.role.clone(),
            artifact_sha256: fixture.artifact_sha256.clone(),
            migration_set_sha256: fixture.migration_set_sha256.clone(),
            config_schema_sha256: fixture.config_schema_sha256.clone(),
            protocol_set_sha256: fixture.protocol_set_sha256.clone(),
            task_queue_sha256: fixture.task_queue_sha256.clone(),
            failure_converter_sha256: fixture.failure_converter_sha256.clone(),
            dependency_evidence_sha256: fixture.dependency_evidence_sha256.clone(),
            health_state: health_state.to_string(),
            reason_code: reason_code.map(str::to_string),
            boot_id: Some(boot_id.to_string()),
            boot_generation: Some(boot_generation),
        }
    }

    fn publish_boot_degraded(
        pool: &DbPool,
        fixture: &ManagedCloudOriginalSourceVerifierStartupRequest,
        boot_id: &str,
        boot_generation: i64,
    ) {
        let heartbeat = record_managed_cloud_runtime_heartbeat(
            pool,
            &boot_heartbeat(
                fixture,
                2,
                "degraded",
                Some("startup"),
                boot_id,
                boot_generation,
            ),
        )
        .expect("publish boot-scoped degraded N2");
        assert_eq!(
            (
                heartbeat.heartbeat_sequence,
                heartbeat.health_state.as_str()
            ),
            (2, "degraded")
        );
    }

    fn ready_generation(
        pool: &DbPool,
        fixture: &ManagedCloudOriginalSourceVerifierStartupRequest,
    ) -> Option<i64> {
        let conn = pool.get().expect("sqlite connection");
        conn.query_row(
            "SELECT ready_boot_generation
               FROM jobs_managed_cloud_source_verifier_boot_generation_heads
              WHERE runtime_instance_id = ?1 AND instance_epoch = ?2",
            params![fixture.runtime_instance_id, fixture.instance_epoch],
            |row| row.get::<_, Option<i64>>(0),
        )
        .expect("generation head row")
    }

    fn boot_binding(
        pool: &DbPool,
        request: &SourceVerifierBootStartupRequest,
    ) -> OriginalSourceVerifierBinding {
        let connection = pool.get().expect("load runtime instance for boot binding");
        let instance = connection
            .query_row(
                &format!(
                    "SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS} FROM {} \
                     WHERE runtime_instance_id=?1 AND instance_epoch=?2",
                    MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES.instances,
                ),
                params![request.runtime_instance_id, request.instance_epoch],
                |row| managed_cloud_runtime_instance_from_sqlite(row).map(|(instance, _)| instance),
            )
            .expect("claimed runtime instance");
        source_verifier_boot_probe_binding(&instance, request).expect("derive boot binding")
    }

    /// Required case: registration response loss followed by an exact retry.
    #[test]
    fn registration_retry_replays_identical_canonical_bytes() {
        let fixture = original_source_verifier_startup_test_fixture();
        let request = db_registration(
            &fixture.request,
            "reg-00000000000000001",
            "boot-0000000000000001",
        );
        let first =
            register_source_verifier_boot(&fixture.pool, &request).expect("first registration");
        assert!(!first.replayed, "first registration must not be a replay");
        assert!(first.boot_generation > 0);
        let retry =
            register_source_verifier_boot(&fixture.pool, &request).expect("registration retry");
        assert!(retry.replayed, "retry must be served as a replay");
        assert_eq!(
            first.canonical_response_base64url, retry.canonical_response_base64url,
            "replay must return byte-identical canonical bytes"
        );
        assert_eq!(first.response_sha256, retry.response_sha256);
        assert_eq!(first.boot_generation, retry.boot_generation);
        assert_eq!(
            first.registered_at_ms, retry.registered_at_ms,
            "replay must return the stored time, not a new one"
        );
    }

    /// Required case: same idempotency token with a changed payload conflicts.
    #[test]
    fn registration_token_with_changed_payload_conflicts() {
        let fixture = original_source_verifier_startup_test_fixture();
        let request = db_registration(
            &fixture.request,
            "reg-00000000000000001",
            "boot-0000000000000001",
        );
        register_source_verifier_boot(&fixture.pool, &request).expect("first registration");
        let mut changed = request.clone();
        changed.boot_id = "boot-0000000000000002".to_string();
        let error = register_source_verifier_boot(&fixture.pool, &changed)
            .expect_err("changed payload under the same token must conflict");
        assert!(
            error.to_string().contains("conflicts"),
            "unexpected error: {error}"
        );
    }

    /// Required cases: restart after committed readiness, and a genuine new boot
    /// receiving a strictly higher generation.
    #[test]
    fn new_boot_receives_strictly_higher_generation() {
        let fixture = original_source_verifier_startup_test_fixture();
        let first = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("first boot");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &first.boot_id,
            first.boot_generation,
        );
        record_source_verifier_boot_startup(
            &fixture.pool,
            &db_startup(
                &fixture.request,
                "sreq-000000000000001",
                &first.boot_id,
                first.boot_generation,
            ),
        )
        .expect("first boot startup");
        let second = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000002",
                "boot-0000000000000002",
            ),
        )
        .expect("second boot");
        assert_eq!(
            second.boot_generation,
            first.boot_generation + 1,
            "a genuine new boot must receive a higher generation"
        );
        assert!(second.boot_generation > first.boot_generation);
    }

    /// Required cases: response loss after the ready transaction commits, and
    /// startup response loss followed by an exact retry.
    #[test]
    fn startup_retry_replays_identical_bytes_without_reprobing() {
        let fixture = original_source_verifier_startup_test_fixture();
        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("registration");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &registration.boot_id,
            registration.boot_generation,
        );
        let request = db_startup(
            &fixture.request,
            "sreq-000000000000001",
            &registration.boot_id,
            registration.boot_generation,
        );
        let first = record_source_verifier_boot_startup(&fixture.pool, &request).expect("startup");
        assert!(!first.replayed);
        assert_eq!(
            (
                first.degraded_heartbeat_sequence,
                first.ready_heartbeat_sequence
            ),
            (2, 3)
        );
        assert_eq!(
            ready_generation(&fixture.pool, &fixture.request),
            Some(registration.boot_generation)
        );
        let retry =
            record_source_verifier_boot_startup(&fixture.pool, &request).expect("startup retry");
        assert!(retry.replayed, "startup retry must replay");
        assert_eq!(
            first.canonical_response_base64url, retry.canonical_response_base64url,
            "startup replay must be byte-identical"
        );
        assert_eq!(first.startup_at_ms, retry.startup_at_ms);
        assert_eq!(
            ready_generation(&fixture.pool, &fixture.request),
            Some(registration.boot_generation),
            "duplicate ready reporting must be a no-op"
        );
    }

    #[test]
    fn startup_retry_after_replacement_boot_is_read_only_and_keeps_old_work_fenced() {
        let fixture = original_source_verifier_startup_test_fixture();
        let first = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-retry-old-00000001",
                "boot-retry-old-000001",
            ),
        )
        .expect("register first tested boot");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &first.boot_id,
            first.boot_generation,
        );
        let request = db_startup(
            &fixture.request,
            "startup-retry-old-000001",
            &first.boot_id,
            first.boot_generation,
        );
        let committed = record_source_verifier_boot_startup(&fixture.pool, &request)
            .expect("commit first boot readiness");
        let old_binding = boot_binding(&fixture.pool, &request);
        let replacement = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-retry-new-00000001",
                "boot-retry-new-000001",
            ),
        )
        .expect("register replacement boot");
        assert_eq!(replacement.boot_generation, first.boot_generation + 1);
        assert!(lease_original_source_verification(&fixture.pool, &old_binding).is_err());

        let tables = [
            "jobs_managed_cloud_source_verifier_boot_generation_heads",
            "jobs_managed_cloud_source_verifier_boot_registrations",
            "jobs_managed_cloud_source_verifier_boot_startup_authorities",
            "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings",
            "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
        ];
        let before: Vec<_> = tables
            .iter()
            .map(|table| snapshot(&fixture.pool, table))
            .collect();
        let replay = record_source_verifier_boot_startup(&fixture.pool, &request)
            .expect("an authenticated exact retry must survive a replacement boot");
        assert!(replay.replayed);
        assert_eq!(committed.canonical_response_base64url, replay.canonical_response_base64url);
        assert_eq!(committed.response_sha256, replay.response_sha256);
        assert_eq!(committed.startup_at_ms, replay.startup_at_ms);

        let mut changed = request.clone();
        changed.degraded_heartbeat_sequence += 1;
        assert!(record_source_verifier_boot_startup(&fixture.pool, &changed)
            .expect_err("a changed payload must not replay")
            .to_string()
            .contains("request id conflicts"));
        let mut fresh = request.clone();
        fresh.startup_request_id = "startup-retry-stale-000001".to_string();
        assert!(record_source_verifier_boot_startup(&fixture.pool, &fresh).is_err());
        let mut unauthenticated = request.clone();
        unauthenticated.session_token = "invalid-runtime-session".to_string();
        assert!(record_source_verifier_boot_startup(&fixture.pool, &unauthenticated).is_err());
        assert!(lease_original_source_verification(&fixture.pool, &old_binding).is_err());
        for (index, table) in tables.iter().enumerate() {
            assert_eq!(before[index], snapshot(&fixture.pool, table), "retry changed {table}");
        }
    }

    /// Required case: same startup token with a changed payload conflicts.
    #[test]
    fn startup_token_with_changed_payload_conflicts() {
        let fixture = original_source_verifier_startup_test_fixture();
        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("registration");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &registration.boot_id,
            registration.boot_generation,
        );
        let request = db_startup(
            &fixture.request,
            "sreq-000000000000001",
            &registration.boot_id,
            registration.boot_generation,
        );
        record_source_verifier_boot_startup(&fixture.pool, &request).expect("startup");
        let mut changed = request.clone();
        changed.boot_id = "boot-0000000000000002".to_string();
        let error = record_source_verifier_boot_startup(&fixture.pool, &changed)
            .expect_err("changed startup payload must conflict");
        assert!(
            error.to_string().contains("conflicts"),
            "unexpected error: {error}"
        );
    }

    /// Required case: a stale boot generation can never win readiness.
    #[test]
    fn stale_generation_startup_cannot_lower_committed_readiness() {
        let fixture = original_source_verifier_startup_test_fixture();
        let first = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("first boot");
        let second = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000002",
                "boot-0000000000000002",
            ),
        )
        .expect("second boot");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &second.boot_id,
            second.boot_generation,
        );
        record_source_verifier_boot_startup(
            &fixture.pool,
            &db_startup(
                &fixture.request,
                "sreq-000000000000002",
                &second.boot_id,
                second.boot_generation,
            ),
        )
        .expect("generation 2 startup");
        assert_eq!(
            ready_generation(&fixture.pool, &fixture.request),
            Some(second.boot_generation)
        );
        let stale = record_source_verifier_boot_startup(
            &fixture.pool,
            &db_startup(
                &fixture.request,
                "sreq-000000000000001",
                &first.boot_id,
                first.boot_generation,
            ),
        );
        assert!(
            stale.is_err(),
            "a stale generation startup must be rejected"
        );
        assert_eq!(
            ready_generation(&fixture.pool, &fixture.request),
            Some(second.boot_generation),
            "committed readiness must not regress"
        );
    }

    /// Required case: concurrent duplicate startup is atomic in SQLite.
    #[test]
    fn concurrent_duplicate_startup_is_atomic_in_sqlite() {
        let fixture = original_source_verifier_startup_test_fixture();
        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("registration");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &registration.boot_id,
            registration.boot_generation,
        );
        let request = db_startup(
            &fixture.request,
            "sreq-000000000000001",
            &registration.boot_id,
            registration.boot_generation,
        );
        let pool_a = fixture.pool.clone();
        let pool_b = fixture.pool.clone();
        let request_a = request.clone();
        let request_b = request.clone();
        let left =
            std::thread::spawn(move || record_source_verifier_boot_startup(&pool_a, &request_a));
        let right =
            std::thread::spawn(move || record_source_verifier_boot_startup(&pool_b, &request_b));
        let left = left.join().expect("left thread");
        let right = right.join().expect("right thread");
        let left = left.expect("left startup");
        let right = right.expect("right startup");
        assert_eq!(
            left.response_sha256, right.response_sha256,
            "concurrent duplicate startup must converge on one authority"
        );
        assert_eq!(left.boot_generation, right.boot_generation);
        let conn = fixture.pool.get().expect("sqlite connection");
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_managed_cloud_source_verifier_boot_startup_authorities",
                [],
                |row| row.get(0),
            )
            .expect("startup count");
        assert_eq!(rows, 1, "exactly one startup row may exist");
    }

    #[test]
    fn unknown_instance_bad_session_and_missing_probe_cannot_make_runtime_ready() {
        let fixture = original_source_verifier_startup_test_fixture();
        let mut unknown = db_registration(
            &fixture.request,
            "reg-00000000000000001",
            "boot-0000000000000001",
        );
        unknown.runtime_instance_id = "unknown-runtime-instance-0001".to_string();
        assert!(register_source_verifier_boot(&fixture.pool, &unknown).is_err());

        let mut bad_session = db_registration(
            &fixture.request,
            "reg-00000000000000002",
            "boot-0000000000000002",
        );
        bad_session.session_token = "not-the-claimed-session-token".to_string();
        assert!(register_source_verifier_boot(&fixture.pool, &bad_session).is_err());

        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000003",
                "boot-0000000000000003",
            ),
        )
        .expect("registration");
        let startup = db_startup(
            &fixture.request,
            "sreq-000000000000003",
            &registration.boot_id,
            registration.boot_generation,
        );
        assert!(record_source_verifier_boot_startup(&fixture.pool, &startup).is_err());
        let connection = fixture
            .pool
            .get()
            .expect("read heartbeat after failed startup");
        let state: (i64, String) = connection
            .query_row(
                "SELECT heartbeat_sequence,health_state
                   FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                  WHERE runtime_instance_id=?1 AND instance_epoch=?2",
                params![
                    fixture.request.runtime_instance_id,
                    fixture.request.instance_epoch
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("runtime heartbeat");
        assert_eq!(state, (1, "degraded".to_string()));
        assert_eq!(ready_generation(&fixture.pool, &fixture.request), None);
    }

    #[test]
    fn late_stale_probe_failure_cannot_downgrade_newer_ready() {
        let fixture = original_source_verifier_startup_test_fixture();
        let first = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000001",
                "boot-0000000000000001",
            ),
        )
        .expect("first boot");
        let second = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-00000000000000002",
                "boot-0000000000000002",
            ),
        )
        .expect("second boot");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &second.boot_id,
            second.boot_generation,
        );
        record_source_verifier_boot_startup(
            &fixture.pool,
            &db_startup(
                &fixture.request,
                "sreq-000000000000002",
                &second.boot_id,
                second.boot_generation,
            ),
        )
        .expect("newest ready");

        let late_failure = boot_heartbeat(
            &fixture.request,
            4,
            "degraded",
            Some("probe_failed"),
            &first.boot_id,
            first.boot_generation,
        );
        assert!(record_managed_cloud_runtime_heartbeat(&fixture.pool, &late_failure).is_err());
        assert_eq!(
            ready_generation(&fixture.pool, &fixture.request),
            Some(second.boot_generation)
        );
    }

    #[test]
    fn current_boot_generation_fences_leases_even_with_a_fresh_old_ready_heartbeat() {
        let fixture = original_source_verifier_startup_test_fixture();
        let first = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-fence-00000000001",
                "boot-fence-000000001",
            ),
        )
        .expect("register first tested boot");
        publish_boot_degraded(
            &fixture.pool,
            &fixture.request,
            &first.boot_id,
            first.boot_generation,
        );
        let first_startup = db_startup(
            &fixture.request,
            "startup-fence-000000001",
            &first.boot_id,
            first.boot_generation,
        );
        record_source_verifier_boot_startup(&fixture.pool, &first_startup)
            .expect("commit first boot ready");
        let first_binding = boot_binding(&fixture.pool, &first_startup);
        assert!(
            lease_original_source_verification(&fixture.pool, &first_binding)
                .expect("current ready boot may poll")
                .is_none()
        );

        let second = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-fence-00000000002",
                "boot-fence-000000002",
            ),
        )
        .expect("register replacement boot");
        assert_eq!(second.boot_generation, first.boot_generation + 1);
        assert!(lease_original_source_verification(&fixture.pool, &first_binding).is_err());

        let degraded = record_managed_cloud_runtime_heartbeat(
            &fixture.pool,
            &boot_heartbeat(
                &fixture.request,
                4,
                "degraded",
                Some("startup"),
                &second.boot_id,
                second.boot_generation,
            ),
        )
        .expect("publish replacement boot degraded N4");
        assert_eq!(degraded.heartbeat_sequence, 4);
        let mut second_startup = db_startup(
            &fixture.request,
            "startup-fence-000000002",
            &second.boot_id,
            second.boot_generation,
        );
        second_startup.degraded_heartbeat_sequence = 4;
        record_source_verifier_boot_startup(&fixture.pool, &second_startup)
            .expect("commit replacement boot ready N5");
        let second_binding = boot_binding(&fixture.pool, &second_startup);
        assert!(lease_original_source_verification(&fixture.pool, &first_binding).is_err());
        assert!(
            lease_original_source_verification(&fixture.pool, &second_binding)
                .expect("current replacement boot may poll")
                .is_none()
        );
        let mut omitted = second_binding;
        omitted.boot_id = None;
        omitted.boot_generation = None;
        assert!(lease_original_source_verification(&fixture.pool, &omitted).is_err());
    }

    #[test]
    fn probe_failed_boot_cannot_be_promoted_by_a_later_startup_request() {
        let fixture = original_source_verifier_startup_test_fixture();
        let registration = register_source_verifier_boot(
            &fixture.pool,
            &db_registration(
                &fixture.request,
                "reg-failed-0000000001",
                "boot-failed-00000001",
            ),
        )
        .expect("register failed boot");
        record_managed_cloud_runtime_heartbeat(
            &fixture.pool,
            &boot_heartbeat(
                &fixture.request,
                2,
                "degraded",
                Some("probe_failed"),
                &registration.boot_id,
                registration.boot_generation,
            ),
        )
        .expect("publish probe failure");
        let request = db_startup(
            &fixture.request,
            "startup-failed-00000001",
            &registration.boot_id,
            registration.boot_generation,
        );
        assert!(record_source_verifier_boot_startup(&fixture.pool, &request).is_err());
        let connection = fixture.pool.get().expect("inspect failed boot state");
        let state: (i64, String, Option<String>, i64) = connection
            .query_row(
                "SELECT heartbeat_sequence,health_state,reason_code,
                        (SELECT COUNT(*) FROM jobs_managed_cloud_source_verifier_boot_startup_authorities)
                   FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                  WHERE runtime_instance_id=?1 AND instance_epoch=?2",
                params![fixture.request.runtime_instance_id, fixture.request.instance_epoch],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("failed boot heartbeat state");
        assert_eq!(
            state,
            (
                2,
                "degraded".to_string(),
                Some("probe_failed".to_string()),
                0
            )
        );
        assert_eq!(ready_generation(&fixture.pool, &fixture.request), None);
    }
}
