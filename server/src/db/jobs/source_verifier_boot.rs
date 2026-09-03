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
    pub startup_request_id: String,
    pub boot_id: String,
    pub runtime_instance_id: String,
    pub instance_epoch: i64,
    pub boot_generation: i64,
    pub worker_id: String,
    pub degraded_heartbeat_sequence: i64,
    pub probe_result: String,
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
    source_verifier_boot_sha256_hex(&source_verifier_boot_canonical_bytes(
        &source_verifier_boot_registration_fields(request),
    ))
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
    fields.insert("probeResult", request.probe_result.clone());
    fields.insert("runtimeInstanceId", request.runtime_instance_id.clone());
    fields.insert("startupRequestId", request.startup_request_id.clone());
    fields.insert("workerId", request.worker_id.clone());
    fields
}

pub(crate) fn source_verifier_boot_startup_request_sha256(
    request: &SourceVerifierBootStartupRequest,
) -> String {
    source_verifier_boot_sha256_hex(&source_verifier_boot_canonical_bytes(
        &source_verifier_boot_startup_fields(request),
    ))
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
    startup_at_ms: i64,
) -> (String, String, String) {
    let mut fields = source_verifier_boot_startup_fields(request);
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
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
                &[
                    &request.runtime_instance_id,
                    &request.instance_epoch,
                    &now,
                ],
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

/// Idempotently records the one-shot startup readiness transition
/// `N degraded -> N+1 ready` for an exact boot generation.
///
/// A repeated `startup_request_id` with the same canonical payload replays the
/// stored bytes without re-running the probe. A different payload conflicts. A
/// stale generation is rejected by the migration fence.
pub(crate) fn record_source_verifier_boot_startup(
    pool: &DbPool,
    request: &SourceVerifierBootStartupRequest,
) -> Result<SourceVerifierBootStartup> {
    let request_sha256 = source_verifier_boot_startup_request_sha256(request);
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_startup_response(request, now);
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
                    request.probe_result,
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
            tx.commit()?;
            Ok(SourceVerifierBootStartup {
                startup_request_id: request.startup_request_id.clone(),
                boot_generation: request.boot_generation,
                degraded_heartbeat_sequence: request.degraded_heartbeat_sequence,
                ready_heartbeat_sequence: request.degraded_heartbeat_sequence + 1,
                probe_result: request.probe_result.clone(),
                canonical_response_base64url,
                response_sha256,
                startup_at_ms: now,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
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
            let (canonical_response_base64url, response_sha256, authority_sha256) =
                source_verifier_boot_startup_response(request, now);
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
                    &request.probe_result,
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
            tx.commit()?;
            Ok(SourceVerifierBootStartup {
                startup_request_id: request.startup_request_id.clone(),
                boot_generation: request.boot_generation,
                degraded_heartbeat_sequence: request.degraded_heartbeat_sequence,
                ready_heartbeat_sequence: ready_sequence,
                probe_result: request.probe_result.clone(),
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

    fn registration_request() -> SourceVerifierBootRegistrationRequest {
        SourceVerifierBootRegistrationRequest {
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
            startup_request_id: "sreq-000000000000001".to_string(),
            boot_id: "boot-0000000000000001".to_string(),
            runtime_instance_id: "ri-1".to_string(),
            instance_epoch: 1,
            boot_generation: 1,
            worker_id: "w-1".to_string(),
            degraded_heartbeat_sequence: 1,
            probe_result: "idle".to_string(),
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
        changed.worker_id = "w-2".to_string();
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
        changed.probe_result = "leaseable".to_string();
        assert_ne!(
            first,
            source_verifier_boot_startup_request_sha256(&changed)
        );
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
        let (response, _, _) = source_verifier_boot_startup_response(&request, 2_000);
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(response.as_bytes())
            .expect("canonical response must be base64url");
        let decoded = String::from_utf8_lossy(&decoded);
        assert!(decoded.contains("readyHeartbeatSequence"));
        assert!(decoded.contains("degradedHeartbeatSequence"));
        let (other, _, _) = source_verifier_boot_startup_response(&request, 3_000);
        assert_ne!(
            response, other,
            "startup time participates in canonical bytes"
        );
    }

    #[test]
    fn different_generations_produce_different_registration_authority() {
        let request = registration_request();
        let (_, _, generation_one) =
            source_verifier_boot_registration_response(&request, 1, 1_000);
        let (_, _, generation_two) =
            source_verifier_boot_registration_response(&request, 2, 1_000);
        assert_ne!(generation_one, generation_two);
    }

    // ---- DB-backed regression coverage (Phase 628 required cases) ----

    fn db_registration(request_id: &str, boot_id: &str) -> SourceVerifierBootRegistrationRequest {
        SourceVerifierBootRegistrationRequest {
            registration_request_id: request_id.to_string(),
            boot_id: boot_id.to_string(),
            runtime_instance_id: "cloud-instance-1234567890".to_string(),
            instance_epoch: 1,
            worker_id: "worker-1234567890abcd".to_string(),
            runtime_identity_sha256: "a".repeat(64),
            activation_sha256: "b".repeat(64),
            manifest_sha256: "c".repeat(64),
            component_id: "jobs-workflows".to_string(),
            role: "original_source_verifier".to_string(),
        }
    }

    fn db_startup(
        request_id: &str,
        boot_id: &str,
        boot_generation: i64,
        degraded_sequence: i64,
    ) -> SourceVerifierBootStartupRequest {
        SourceVerifierBootStartupRequest {
            startup_request_id: request_id.to_string(),
            boot_id: boot_id.to_string(),
            runtime_instance_id: "cloud-instance-1234567890".to_string(),
            instance_epoch: 1,
            boot_generation,
            worker_id: "worker-1234567890abcd".to_string(),
            degraded_heartbeat_sequence: degraded_sequence,
            probe_result: "idle".to_string(),
        }
    }

    fn ready_generation(pool: &DbPool) -> Option<i64> {
        let conn = pool.get().expect("sqlite connection");
        conn.query_row(
            "SELECT ready_boot_generation
               FROM jobs_managed_cloud_source_verifier_boot_generation_heads
              WHERE runtime_instance_id = ?1 AND instance_epoch = ?2",
            params!["cloud-instance-1234567890", 1_i64],
            |row| row.get::<_, Option<i64>>(0),
        )
        .expect("generation head row")
    }

    /// Required case: registration response loss followed by an exact retry.
    #[test]
    fn registration_retry_replays_identical_canonical_bytes() {
        let pool = super::tests::test_pool();
        let request = db_registration("reg-00000000000000001", "boot-0000000000000001");
        let first = register_source_verifier_boot(&pool, &request).expect("first registration");
        assert!(!first.replayed, "first registration must not be a replay");
        assert_eq!(first.boot_generation, 1);
        let retry = register_source_verifier_boot(&pool, &request).expect("registration retry");
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
        let pool = super::tests::test_pool();
        let request = db_registration("reg-00000000000000001", "boot-0000000000000001");
        register_source_verifier_boot(&pool, &request).expect("first registration");
        let mut changed = request.clone();
        changed.worker_id = "worker-9999999999zzzz".to_string();
        let error = register_source_verifier_boot(&pool, &changed)
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
        let pool = super::tests::test_pool();
        let first = register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000001", "boot-0000000000000001"),
        )
        .expect("first boot");
        let second = register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000002", "boot-0000000000000002"),
        )
        .expect("second boot");
        assert_eq!(first.boot_generation, 1);
        assert_eq!(
            second.boot_generation, 2,
            "a genuine new boot must receive a higher generation"
        );
        assert!(second.boot_generation > first.boot_generation);
    }

    /// Required cases: response loss after the ready transaction commits, and
    /// startup response loss followed by an exact retry.
    #[test]
    fn startup_retry_replays_identical_bytes_without_reprobing() {
        let pool = super::tests::test_pool();
        register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000001", "boot-0000000000000001"),
        )
        .expect("registration");
        let request = db_startup("sreq-000000000000001", "boot-0000000000000001", 1, 1);
        let first = record_source_verifier_boot_startup(&pool, &request).expect("startup");
        assert!(!first.replayed);
        assert_eq!(first.degraded_heartbeat_sequence, 1);
        assert_eq!(first.ready_heartbeat_sequence, 2);
        assert_eq!(ready_generation(&pool), Some(1));
        let retry = record_source_verifier_boot_startup(&pool, &request).expect("startup retry");
        assert!(retry.replayed, "startup retry must replay");
        assert_eq!(
            first.canonical_response_base64url, retry.canonical_response_base64url,
            "startup replay must be byte-identical"
        );
        assert_eq!(first.startup_at_ms, retry.startup_at_ms);
        assert_eq!(
            ready_generation(&pool),
            Some(1),
            "duplicate ready reporting must be a no-op"
        );
    }

    /// Required case: same startup token with a changed payload conflicts.
    #[test]
    fn startup_token_with_changed_payload_conflicts() {
        let pool = super::tests::test_pool();
        register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000001", "boot-0000000000000001"),
        )
        .expect("registration");
        let request = db_startup("sreq-000000000000001", "boot-0000000000000001", 1, 1);
        record_source_verifier_boot_startup(&pool, &request).expect("startup");
        let mut changed = request.clone();
        changed.probe_result = "leaseable".to_string();
        let error = record_source_verifier_boot_startup(&pool, &changed)
            .expect_err("changed startup payload must conflict");
        assert!(
            error.to_string().contains("conflicts"),
            "unexpected error: {error}"
        );
    }

    /// Required case: a stale boot generation can never win readiness.
    #[test]
    fn stale_generation_startup_cannot_lower_committed_readiness() {
        let pool = super::tests::test_pool();
        register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000001", "boot-0000000000000001"),
        )
        .expect("first boot");
        register_source_verifier_boot(
            &pool,
            &db_registration("reg-00000000000000002", "boot-0000000000000002"),
        )
        .expect("second boot");
        record_source_verifier_boot_startup(
            &pool,
            &db_startup("sreq-000000000000002", "boot-0000000000000002", 2, 5),
        )
        .expect("generation 2 startup");
        assert_eq!(ready_generation(&pool), Some(2));
        let stale = record_source_verifier_boot_startup(
            &pool,
            &db_startup("sreq-000000000000001", "boot-0000000000000001", 1, 1),
        );
        assert!(
            stale.is_err(),
            "a stale generation startup must be rejected"
        );
        assert_eq!(
            ready_generation(&pool),
            Some(2),
            "committed readiness must not regress"
        );
    }

    /// Required case: concurrent duplicate startup is atomic in SQLite.
    #[test]
    fn concurrent_duplicate_registration_is_atomic_in_sqlite() {
        let pool = super::tests::test_pool();
        let request = db_registration("reg-00000000000000001", "boot-0000000000000001");
        let pool_a = pool.clone();
        let pool_b = pool.clone();
        let request_a = request.clone();
        let request_b = request.clone();
        let left = std::thread::spawn(move || register_source_verifier_boot(&pool_a, &request_a));
        let right = std::thread::spawn(move || register_source_verifier_boot(&pool_b, &request_b));
        let left = left.join().expect("left thread");
        let right = right.join().expect("right thread");
        let left = left.expect("left registration");
        let right = right.expect("right registration");
        assert_eq!(
            left.response_sha256, right.response_sha256,
            "concurrent duplicate registration must converge on one authority"
        );
        assert_eq!(left.boot_generation, right.boot_generation);
        assert_eq!(left.boot_generation, 1, "no generation may be skipped");
        let conn = pool.get().expect("sqlite connection");
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_managed_cloud_source_verifier_boot_registrations",
                [],
                |row| row.get(0),
            )
            .expect("registration count");
        assert_eq!(rows, 1, "exactly one registration row may exist");
    }

}
