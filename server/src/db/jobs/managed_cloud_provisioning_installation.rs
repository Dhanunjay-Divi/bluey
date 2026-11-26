//! Provisioning bootstrap metadata. Allocation is not physical queue ownership.

use super::managed_cloud_provisioning_persistence::ProvisioningDocument;
use super::managed_cloud_provisioning_protocol::{AuthorityBinding, AuthorityKind, Purpose};
use super::managed_cloud_provisioning_runtime::{
    self as runtime, ControlTransaction, ProvisioningProcessContext,
};
use super::*;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AllocateProvisioningQueueRequest {
    pub issuance_ref: String,
    pub scope: ManagedCloudScope,
    pub namespace: String,
    pub task_queue: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AllocatedProvisioningQueue {
    pub queue_generation_id: String,
    pub issuance_ref: String,
    pub scope: ManagedCloudScope,
    pub namespace: String,
    pub task_queue: String,
    pub task_queue_sha256: String,
    pub created_at_ms: i64,
    pub replayed: bool,
}

fn canonical_queue_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.trim() == value
        && !value.starts_with('\u{feff}')
        && !value.ends_with('\u{feff}')
        && !value.contains('\u{fffd}')
        && !value.chars().any(char::is_control)
        && value.nfc().eq(value.chars())
}

/// Server allocation records immutable intent only; it does not inspect Temporal.
pub(crate) fn allocate_provisioning_queue(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    request: &AllocateProvisioningQueueRequest,
    recorded_by: &str,
) -> ManagedCloudResult<AllocatedProvisioningQueue> {
    if !managed_cloud_route_id(&request.issuance_ref)
        || !managed_cloud_actor(recorded_by)
        || !canonical_queue_text(&request.namespace)
        || !canonical_queue_text(&request.task_queue)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    validate_managed_cloud_scope(&request.scope)?;
    process.permits(&AuthorityBinding {
        authority_kind: AuthorityKind::ProvisioningV4,
        purpose: Purpose::Production,
        context_sha256: None,
        queue_generation_id: request.issuance_ref.clone(),
    })?;
    let digest = managed_cloud_task_queue_sha256(&request.namespace, &request.task_queue)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let existing = tx.query_row(
                "SELECT queue_generation_id,environment,region,channel,purpose,namespace,\
                 task_queue,task_queue_sha256,created_at_ms FROM jobs_managed_cloud_queue_generations \
                 WHERE issuance_ref=?1", [&request.issuance_ref], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?, row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?, row.get::<_, String>(7)?, row.get::<_, i64>(8)?))
                },
            ).optional().map_err(managed_cloud_storage)?;
            let (id, now, replayed) = if let Some(row) = existing {
                if row.1 != request.scope.environment
                    || row.2 != request.scope.region
                    || row.3 != request.scope.channel
                    || row.4 != "production"
                    || row.5 != request.namespace
                    || row.6 != request.task_queue
                    || row.7 != digest
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                (row.0, row.8, true)
            } else {
                let now = managed_cloud_db_now_sqlite(&tx)?;
                let id = format!("q_{}", uuid::Uuid::new_v4().simple());
                let inserted = tx
                    .execute(
                        "INSERT INTO jobs_managed_cloud_queue_generations \
                     (queue_generation_id,issuance_ref,environment,region,channel,purpose,\
                     namespace,task_queue,task_queue_sha256,created_at_ms) \
                     VALUES (?1,?2,?3,?4,?5,'production',?6,?7,?8,?9) ON CONFLICT DO NOTHING",
                        params![
                            id,
                            request.issuance_ref,
                            request.scope.environment,
                            request.scope.region,
                            request.scope.channel,
                            request.namespace,
                            request.task_queue,
                            digest,
                            now
                        ],
                    )
                    .map_err(managed_cloud_storage)?;
                if inserted != 1 {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                (id, now, false)
            };
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(AllocatedProvisioningQueue {
                queue_generation_id: id,
                issuance_ref: request.issuance_ref.clone(),
                scope: request.scope.clone(),
                namespace: request.namespace.clone(),
                task_queue: request.task_queue.clone(),
                task_queue_sha256: digest,
                created_at_ms: now,
                replayed,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            lock_operational_hold_shared_postgres_tx(&mut tx)
                .map_err(map_managed_cloud_operational_error)?;
            lock_managed_cloud_release_registry_postgres_tx(&mut tx, &request.scope)?;
            let existing = tx.query_opt(
                "SELECT queue_generation_id,environment,region,channel,purpose,namespace,\
                 task_queue,task_queue_sha256,created_at_ms FROM jobs_managed_cloud_queue_generations \
                 WHERE issuance_ref=$1 FOR UPDATE", &[&request.issuance_ref],
            ).map_err(managed_cloud_storage)?;
            let (id, now, replayed) = if let Some(row) = existing {
                if row.get::<_, String>(1) != request.scope.environment
                    || row.get::<_, String>(2) != request.scope.region
                    || row.get::<_, String>(3) != request.scope.channel
                    || row.get::<_, String>(4) != "production"
                    || row.get::<_, String>(5) != request.namespace
                    || row.get::<_, String>(6) != request.task_queue
                    || row.get::<_, String>(7) != digest
                {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                (row.get::<_, String>(0), row.get::<_, i64>(8), true)
            } else {
                let now = managed_cloud_db_now_postgres(&mut tx)?;
                let id = format!("q_{}", uuid::Uuid::new_v4().simple());
                let inserted = tx
                    .execute(
                        "INSERT INTO jobs_managed_cloud_queue_generations \
                     (queue_generation_id,issuance_ref,environment,region,channel,purpose,\
                     namespace,task_queue,task_queue_sha256,created_at_ms) \
                     VALUES ($1,$2,$3,$4,$5,'production',$6,$7,$8,$9) ON CONFLICT DO NOTHING",
                        &[
                            &id,
                            &request.issuance_ref,
                            &request.scope.environment,
                            &request.scope.region,
                            &request.scope.channel,
                            &request.namespace,
                            &request.task_queue,
                            &digest,
                            &now,
                        ],
                    )
                    .map_err(managed_cloud_storage)?;
                if inserted != 1 {
                    return Err(ManagedCloudRegistryError::IdentityConflict);
                }
                (id, now, false)
            };
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(AllocatedProvisioningQueue {
                queue_generation_id: id,
                issuance_ref: request.issuance_ref.clone(),
                scope: request.scope.clone(),
                namespace: request.namespace.clone(),
                task_queue: request.task_queue.clone(),
                task_queue_sha256: digest,
                created_at_ms: now,
                replayed,
            })
        }
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstallProvisioningRequest {
    pub envelope: ManagedCloudAuthorityEnvelope,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledProvisioningHead {
    pub activation_sha256: String,
    pub transition_sha256: String,
    pub head_revision: i64,
    pub replayed: bool,
}

// The following parameters are internal fixed SQL values, not caller SQL.
enum InstallValue {
    Text(String),
    Integer(i64),
    OptionalText(Option<String>),
}

impl rusqlite::ToSql for InstallValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Self::Text(value) => rusqlite::ToSql::to_sql(value),
            Self::Integer(value) => rusqlite::ToSql::to_sql(value),
            Self::OptionalText(value) => rusqlite::ToSql::to_sql(value),
        }
    }
}

impl InstallValue {
    fn pg(&self) -> &(dyn postgres::types::ToSql + Sync) {
        match self {
            Self::Text(value) => value,
            Self::Integer(value) => value,
            Self::OptionalText(value) => value,
        }
    }
}

fn install_execute(
    tx: &mut ControlTransaction<'_>,
    sql: &str,
    values: &[InstallValue],
) -> ManagedCloudResult<u64> {
    match tx {
        ControlTransaction::Sqlite(tx) => tx
            .execute(sql, rusqlite::params_from_iter(values.iter()))
            .map(|count| count as u64)
            .map_err(managed_cloud_storage),
        ControlTransaction::Postgres(tx) => tx
            .execute(
                sql,
                &values.iter().map(InstallValue::pg).collect::<Vec<_>>(),
            )
            .map_err(managed_cloud_storage),
    }
}

fn install_pair(
    tx: &mut ControlTransaction<'_>,
    sql: &str,
    values: &[InstallValue],
) -> ManagedCloudResult<Option<(String, String)>> {
    match tx {
        ControlTransaction::Sqlite(tx) => tx
            .query_row(sql, rusqlite::params_from_iter(values.iter()), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(managed_cloud_storage),
        ControlTransaction::Postgres(tx) => tx
            .query_opt(
                sql,
                &values.iter().map(InstallValue::pg).collect::<Vec<_>>(),
            )
            .map(|row| row.map(|row| (row.get(0), row.get(1))))
            .map_err(managed_cloud_storage),
    }
}

/// Installs only a signed, nonserving control head. The queue row is allocation
/// intent, not evidence that a physical queue or a poller exists.
pub(crate) fn install_provisioning_head(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    request: &InstallProvisioningRequest,
    recorded_by: &str,
) -> ManagedCloudResult<InstalledProvisioningHead> {
    use InstallValue::{Integer as I, OptionalText as O, Text as T};
    if !managed_cloud_actor(recorded_by) {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    let (bytes, signature_bytes) = managed_cloud_decode_envelope(&request.envelope)?;
    let doc: ProvisioningDocument = managed_cloud_parse_canonical(&bytes)?;
    let signature: ManagedCloudSignatureSetAuthority =
        managed_cloud_parse_canonical(&signature_bytes)?;
    validate_managed_cloud_signature_set(&signature)?;
    validate_managed_cloud_scope(&doc.scope)?;
    process.permits(&doc.authority_binding)?;
    super::managed_cloud_provisioning_protocol::canonical_binding_bytes(&doc.authority_binding)?;
    if doc.authority_binding.authority_kind != AuthorityKind::ProvisioningV4
        || doc.authority_binding.purpose != Purpose::Production
        || doc.authority_binding.context_sha256.is_some()
        || doc.version != 4
        || doc.audience != "bluey-jobs-managed-cloud-provisioning-v4"
        || doc.kind != "provisioned_nonserving"
        || !managed_cloud_token(&doc.activation_id, 128)
        || !managed_cloud_safe_integer(doc.activation_generation, true)
        || !managed_cloud_safe_integer(doc.trust_generation, true)
        || !managed_cloud_safe_integer(doc.channel_sequence, true)
        || !managed_cloud_safe_integer(doc.expected_head_revision, false)
        || doc.expected_head_revision >= MANAGED_CLOUD_MAX_SAFE_INTEGER
        || (doc.expected_head_revision == 0) != doc.expected_transition_sha256.is_none()
        || (doc.expected_head_revision == 0) != doc.predecessor_activation_sha256.is_none()
        || doc
            .expected_transition_sha256
            .as_ref()
            .is_some_and(|v| !managed_cloud_hex64(v))
        || doc
            .predecessor_activation_sha256
            .as_ref()
            .is_some_and(|v| !managed_cloud_hex64(v))
        || ![
            &doc.manifest_sha256,
            &doc.manifest_signature_set_sha256,
            &doc.cohort_sha256,
            &doc.cohort_signature_set_sha256,
            &doc.intended_feature_authority_sha256,
            &doc.temporal_namespace_sha256,
            &doc.storage_config_sha256,
            &doc.task_queue_sha256,
            &doc.failure_converter_sha256,
        ]
        .iter()
        .all(|v| managed_cloud_hex64(v))
        || managed_cloud_digest(&doc.intended_feature_authority)?
            != doc.intended_feature_authority_sha256
        || managed_cloud_required_roles_for_feature(&doc.intended_feature_authority).len() != 9
        || !doc.recovery_acceptances.is_empty()
        || ![doc.issued_at_ms, doc.not_before_ms, doc.expires_at_ms]
            .iter()
            .all(|v| managed_cloud_safe_integer(*v, false))
        || doc.not_before_ms < doc.issued_at_ms
        || doc.expires_at_ms <= doc.not_before_ms
        || !(5_000..=300_000).contains(&doc.heartbeat_ttl_ms)
        || !(0..=100_000).contains(&doc.maximum_inflight)
        || !(0..=1_000_000).contains(&doc.maximum_daily_admissions)
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    let sha = managed_cloud_sha256(&bytes);
    let signature_sha = managed_cloud_sha256(&signature_bytes);
    runtime::transaction(pool, |tx| {
        if let ControlTransaction::Postgres(pg) = tx {
            lock_managed_cloud_release_registry_postgres_tx(pg, &doc.scope)?;
        }
        let now = tx.now()?;
        // The provisioned intent must be exactly the signed manifest's feature
        // intent. The runtime validation below independently verifies signatures.
        let (manifest, manifest_signature) = install_pair(tx,
            "SELECT canonical_manifest_base64url,authorization_signature_set_sha256 FROM jobs_managed_cloud_manifests WHERE manifest_sha256=$1",
            &[T(doc.manifest_sha256.clone())])?.ok_or(ManagedCloudRegistryError::NotFound)?;
        let release: ManagedCloudReleaseAuthority =
            managed_cloud_parse_canonical(&managed_cloud_decode_base64url(&manifest)?)?;
        if release.feature_authority != doc.intended_feature_authority
            || manifest_signature != doc.manifest_signature_set_sha256
        {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        let existing = install_pair(tx,
            "SELECT canonical_activation_base64url,authorization_signature_set_sha256 FROM jobs_managed_cloud_activations WHERE activation_sha256=$1",
            &[T(sha.clone())])?;
        if let Some((canonical, stored_signature)) = existing {
            if canonical != request.envelope.canonical_base64url
                || stored_signature != signature_sha
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
            let transition = match tx {
                ControlTransaction::Sqlite(db) => {
                    sqlite_managed_cloud_transition_by_authority(db, &sha)?
                }
                ControlTransaction::Postgres(db) => {
                    postgres_managed_cloud_transition_by_authority(db, &sha)?
                }
            }
            .ok_or(ManagedCloudRegistryError::CompareAndSwapConflict)?;
            let current = install_pair(tx,
                "SELECT current_activation_sha256,current_transition_sha256 FROM jobs_managed_cloud_heads WHERE environment=$1 AND region=$2 AND channel=$3",
                &[T(doc.scope.environment.clone()), T(doc.scope.region.clone()), T(doc.scope.channel.clone())])?;
            if current != Some((sha.clone(), transition.1.clone())) {
                return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
            }
            runtime::validate_installed_provisioning_authority(tx, process, &doc.scope, now)?;
            return Ok(InstalledProvisioningHead {
                activation_sha256: sha.clone(),
                transition_sha256: transition.1,
                head_revision: transition.0.head_revision,
                replayed: true,
            });
        }
        let previous = match tx {
            ControlTransaction::Sqlite(db) => db.query_row(
                "SELECT head_revision,current_transition_sha256,current_activation_sha256,current_manifest_sha256,current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment=?1 AND region=?2 AND channel=?3",
                params![doc.scope.environment,doc.scope.region,doc.scope.channel],
                |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,i64>(4)?,row.get::<_,i64>(5)?)))
                .optional().map_err(managed_cloud_storage)?,
            ControlTransaction::Postgres(db) => db.query_opt(
                "SELECT head_revision,current_transition_sha256,current_activation_sha256,current_manifest_sha256,current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment=$1 AND region=$2 AND channel=$3 FOR UPDATE",
                &[&doc.scope.environment,&doc.scope.region,&doc.scope.channel]).map_err(managed_cloud_storage)?
                .map(|row| (row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5))),
        };
        let matches = match previous.as_ref() {
            None => doc.expected_head_revision == 0 && doc.channel_sequence == 1,
            Some((rev, transition, activation, _, generation, sequence)) => {
                doc.expected_head_revision == *rev
                    && doc.expected_transition_sha256.as_ref() == Some(transition)
                    && doc.predecessor_activation_sha256.as_ref() == Some(activation)
                    && (doc.trust_generation > *generation
                        || (doc.trust_generation == *generation
                            && doc.channel_sequence > *sequence))
            }
        };
        if !matches {
            return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
        }
        let signature_exists = match tx {
            ControlTransaction::Sqlite(db) => require_sqlite_managed_cloud_signature_set_replay(
                db,
                &signature,
                &signature_sha,
                &request.envelope.signature_set_base64url,
            )?,
            ControlTransaction::Postgres(db) => {
                require_postgres_managed_cloud_signature_set_replay(
                    db,
                    &signature,
                    &signature_sha,
                    &request.envelope.signature_set_base64url,
                )?
            }
        };
        if !signature_exists {
            match tx {
                ControlTransaction::Sqlite(db) => insert_sqlite_managed_cloud_signature_set(
                    db,
                    &signature,
                    &signature_sha,
                    &request.envelope.signature_set_base64url,
                    recorded_by,
                    now,
                )?,
                ControlTransaction::Postgres(db) => insert_postgres_managed_cloud_signature_set(
                    db,
                    &signature,
                    &signature_sha,
                    &request.envelope.signature_set_base64url,
                    recorded_by,
                    now,
                )?,
            }
        }
        let values = vec![
            T(sha.clone()),
            T(doc.activation_id.clone()),
            I(doc.activation_generation),
            I(doc.trust_generation),
            T(doc.scope.environment.clone()),
            T(doc.scope.region.clone()),
            T(doc.scope.channel.clone()),
            I(doc.channel_sequence),
            I(doc.expected_head_revision),
            O(doc.expected_transition_sha256.clone()),
            O(doc.predecessor_activation_sha256.clone()),
            T(doc.manifest_sha256.clone()),
            T(doc.manifest_signature_set_sha256.clone()),
            T(doc.cohort_sha256.clone()),
            T(doc.cohort_signature_set_sha256.clone()),
            T(doc.intended_feature_authority_sha256.clone()),
            T(doc.temporal_namespace_sha256.clone()),
            T(doc.storage_config_sha256.clone()),
            T(doc.task_queue_sha256.clone()),
            T(doc.failure_converter_sha256.clone()),
            I(doc.maximum_inflight),
            I(doc.maximum_daily_admissions),
            I(doc.heartbeat_ttl_ms),
            T(request.envelope.canonical_base64url.clone()),
            T(signature_sha.clone()),
            I(doc.issued_at_ms),
            I(doc.not_before_ms),
            I(doc.expires_at_ms),
            T(recorded_by.to_owned()),
            I(now),
            T(doc.authority_binding.queue_generation_id.clone()),
        ];
        if install_execute(tx, "INSERT INTO jobs_managed_cloud_activations (activation_sha256,activation_id,activation_generation,trust_generation,environment,region,channel,channel_sequence,expected_head_revision,expected_transition_sha256,predecessor_activation_sha256,manifest_sha256,manifest_signature_set_sha256,cohort_sha256,cohort_signature_set_sha256,feature_authority_sha256,cloud_distribution_enabled,workflow_command_dispatch_enabled,workflow_cleanup_enabled,direct_discovery_enabled,global_discovery_enabled,source_verification_enabled,runner_fleet_evidence_sha256,cleanup_authority_sha256,temporal_namespace_sha256,storage_config_sha256,task_queue_sha256,failure_converter_sha256,canary_evidence_sha256,portal_readback_evidence_sha256,portal_readback_at_ms,portal_readback_ttl_ms,maximum_inflight,maximum_daily_admissions,requirement_count,heartbeat_ttl_ms,recovery_acceptance_count,canonical_activation_base64url,authorization_signature_set_sha256,issued_at_ms,not_before_ms,expires_at_ms,recorded_by,recorded_at_ms,authority_kind,purpose,context_sha256,queue_generation_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,FALSE,FALSE,FALSE,FALSE,FALSE,FALSE,NULL,NULL,$17,$18,$19,$20,NULL,NULL,NULL,NULL,$21,$22,9,$23,0,$24,$25,$26,$27,$28,$29,$30,'provisioning_v4','production',NULL,$31)", &values)? != 1 {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        for (ordinal, role) in
            managed_cloud_required_roles_for_feature(&doc.intended_feature_authority)
                .iter()
                .enumerate()
        {
            let (component, artifact) = install_pair(tx,
                "SELECT capability.component_id,component.artifact_sha256 FROM jobs_managed_cloud_manifest_capabilities capability JOIN jobs_managed_cloud_manifest_components component ON component.manifest_sha256=capability.manifest_sha256 AND component.component_id=capability.component_id WHERE capability.manifest_sha256=$1 AND capability.capability=$2",
                &[T(doc.manifest_sha256.clone()),T((*role).to_string())])?.ok_or(ManagedCloudRegistryError::IdentityConflict)?;
            let digest = super::managed_cloud_provisioning_protocol::dependency_evidence_sha256_v2(
                &super::managed_cloud_provisioning_protocol::RuntimeDependencyBinding {
                    role,
                    activation_sha256: &sha,
                    manifest_sha256: &doc.manifest_sha256,
                    component_id: &component,
                    artifact_sha256: &artifact,
                    task_queue_sha256: &doc.task_queue_sha256,
                    failure_converter_sha256: &doc.failure_converter_sha256,
                },
                &doc.authority_binding,
            )?;
            if install_execute(tx, "INSERT INTO jobs_managed_cloud_activation_requirements(activation_sha256,role,minimum_ready_instances,heartbeat_ttl_ms,dependency_evidence_sha256,ordinal) VALUES($1,$2,1,$3,$4,$5)",
                &[T(sha.clone()),T((*role).to_string()),I(doc.heartbeat_ttl_ms),T(digest),I(ordinal as i64)])? != 1 {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
        }
        let transition = ManagedCloudHeadTransitionAuthority {
            version: 1,
            audience: MANAGED_CLOUD_TRANSITION_AUDIENCE.into(),
            scope: doc.scope.clone(),
            head_revision: doc.expected_head_revision + 1,
            previous_head_revision: doc.expected_head_revision,
            previous_transition_sha256: previous.as_ref().map(|v| v.1.clone()),
            previous_activation_sha256: previous.as_ref().map(|v| v.2.clone()),
            previous_manifest_sha256: previous.as_ref().map(|v| v.3.clone()),
            previous_trust_generation: previous.as_ref().map(|v| v.4),
            previous_channel_sequence: previous.as_ref().map(|v| v.5),
            next_activation_sha256: sha.clone(),
            next_manifest_sha256: doc.manifest_sha256.clone(),
            next_trust_generation: doc.trust_generation,
            next_channel_sequence: doc.channel_sequence,
            transition_kind: "provisioning".into(),
            authority_sha256: sha.clone(),
            rollback_authority_sha256: None,
            recorded_at_ms: now,
        };
        let transition_sha = managed_cloud_digest(&transition)?;
        match tx {
            ControlTransaction::Sqlite(db) => insert_sqlite_managed_cloud_head_transition(
                db,
                &transition,
                &transition_sha,
                recorded_by,
            )?,
            ControlTransaction::Postgres(db) => insert_postgres_managed_cloud_head_transition(
                db,
                &transition,
                &transition_sha,
                recorded_by,
            )?,
        }
        runtime::validate_installed_provisioning_authority(tx, process, &doc.scope, now)?;
        Ok(InstalledProvisioningHead {
            activation_sha256: sha.clone(),
            transition_sha256: transition_sha,
            head_revision: transition.head_revision,
            replayed: false,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::super::managed_cloud_release_authority_tests as legacy_tests;
    use super::*;

    fn signed_fixture(pool: &DbPool) -> (ProvisioningDocument, InstallProvisioningRequest) {
        let now = crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap(),
            DbPool::Postgres(_) => pool
                .get_pg()
                .unwrap()
                .query_one(
                    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
                    &[],
                )
                .unwrap()
                .get(0),
        });
        let issued = now - 5_000;
        let expires = now + 600_000;
        let roles = [
            "general_promotion",
            "incident",
            "promotion",
            "release",
            "root",
        ];
        let policy = ManagedCloudTrustPolicyAuthority {
            version: 1,
            audience: MANAGED_CLOUD_TRUST_POLICY_AUDIENCE.into(),
            policy_id: "phase656-policy".into(),
            trust_generation: 1,
            predecessor_policy_sha256: None,
            issued_at_ms: issued,
            valid_from_ms: issued,
            expires_at_ms: expires,
            roles: roles
                .iter()
                .map(|role| ManagedCloudTrustRoleAuthority {
                    role: (*role).into(),
                    threshold: 1,
                })
                .collect(),
            keys: roles
                .iter()
                .map(|role| ManagedCloudTrustKeyAuthority {
                    key_id: legacy_tests::managed_cloud_test_key_id(role),
                    role: (*role).into(),
                    public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                        legacy_tests::managed_cloud_test_signing_key(role)
                            .verifying_key()
                            .to_bytes(),
                    ),
                    state: "active".into(),
                    valid_from_ms: issued,
                    valid_until_ms: expires,
                    minimum_trust_generation: 1,
                    maximum_trust_generation: 1,
                })
                .collect(),
        };
        let root_key = legacy_tests::managed_cloud_test_signing_key("root");
        let root = ManagedCloudRootTrustAnchor {
            version: 1,
            audience: MANAGED_CLOUD_ROOT_ANCHOR_AUDIENCE.into(),
            threshold: 1,
            keys: vec![ManagedCloudRootTrustKey {
                key_id: legacy_tests::managed_cloud_test_key_id("root"),
                public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(root_key.verifying_key().to_bytes()),
                valid_from_ms: issued,
                valid_until_ms: expires,
            }],
        };
        let policy_envelope = legacy_tests::managed_cloud_test_signed_envelope(
            &policy,
            MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
            "root",
            now - 1_000,
        );
        import_managed_cloud_trust_policy_with_bootstrap_anchor(
            pool,
            &policy_envelope.envelope,
            "phase656-test",
            Some(&root),
        )
        .unwrap();
        let (release, release_request) = legacy_tests::managed_cloud_test_release_import(issued, 3);
        let imported_release =
            import_managed_cloud_release(pool, &release_request, "phase656-test").unwrap();
        let failure_converter = crate::db::run_blocking_db(|| {
            match pool {
            DbPool::Sqlite(_) => pool.get().unwrap().query_row("SELECT failure_converter_sha256 FROM jobs_managed_cloud_manifests WHERE manifest_sha256=?1", [&imported_release.authority_sha256], |row| row.get::<_,String>(0)).unwrap(),
            DbPool::Postgres(_) => pool.get_pg().unwrap().query_one("SELECT failure_converter_sha256 FROM jobs_managed_cloud_manifests WHERE manifest_sha256=$1", &[&imported_release.authority_sha256]).unwrap().get(0),
        }
        });
        let scope = request().scope;
        let cohort = ManagedCloudCohortAuthority {
            version: 1,
            audience: MANAGED_CLOUD_COHORT_AUDIENCE.into(),
            cohort_id: "phase656-cohort".into(),
            cohort_generation: 1,
            trust_generation: 1,
            scope: scope.clone(),
            rollout_mode: "all_eligible_accounts".into(),
            account_id_sha256s: Vec::new(),
            approval_ref: "phase656-approval".into(),
            issued_at_ms: issued,
            not_before_ms: issued,
            expires_at_ms: expires,
        };
        let cohort_envelope = legacy_tests::managed_cloud_test_signed_envelope(
            &cohort,
            MANAGED_CLOUD_COHORT_AUDIENCE,
            "general_promotion",
            now - 1_000,
        );
        let imported_cohort = import_managed_cloud_cohort(
            pool,
            &ManagedCloudCohortImportRequest {
                envelope: cohort_envelope.envelope,
                account_ids: Vec::new(),
            },
            "phase656-test",
        )
        .unwrap();
        let queue = allocate_provisioning_queue(
            pool,
            &ProvisioningProcessContext::production(),
            &request(),
            "phase656-test",
        )
        .unwrap();
        let doc = ProvisioningDocument {
            version: 4,
            audience: "bluey-jobs-managed-cloud-provisioning-v4".into(),
            kind: "provisioned_nonserving".into(),
            activation_id: "phase656-activation".into(),
            activation_generation: 1,
            trust_generation: 1,
            scope,
            channel_sequence: 1,
            expected_head_revision: 0,
            expected_transition_sha256: None,
            predecessor_activation_sha256: None,
            manifest_sha256: imported_release.authority_sha256,
            manifest_signature_set_sha256: imported_release.signature_set_sha256,
            cohort_sha256: imported_cohort.authority_sha256,
            cohort_signature_set_sha256: imported_cohort.signature_set_sha256,
            intended_feature_authority_sha256: release.feature_authority_sha256.clone(),
            intended_feature_authority: release.feature_authority,
            temporal_namespace_sha256: managed_cloud_sha256(request().namespace.as_bytes()),
            storage_config_sha256: "5".repeat(64),
            task_queue_sha256: queue.task_queue_sha256,
            failure_converter_sha256: failure_converter,
            maximum_inflight: 8,
            maximum_daily_admissions: 100,
            heartbeat_ttl_ms: 30_000,
            recovery_acceptances: Vec::new(),
            issued_at_ms: issued,
            not_before_ms: issued,
            expires_at_ms: now + 120_000,
            authority_binding: AuthorityBinding {
                authority_kind: AuthorityKind::ProvisioningV4,
                purpose: Purpose::Production,
                context_sha256: None,
                queue_generation_id: queue.queue_generation_id,
            },
        };
        let envelope = legacy_tests::managed_cloud_test_signed_envelope(
            &doc,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            now - 1_000,
        )
        .envelope;
        (doc, InstallProvisioningRequest { envelope })
    }

    fn assert_signed_first_head(pool: &DbPool) {
        let (doc, input) = signed_fixture(pool);
        let process = ProvisioningProcessContext::production();
        let counts = || {
            crate::db::run_blocking_db(|| {
                match pool {
            DbPool::Sqlite(_) => pool.get().unwrap().query_row("SELECT (SELECT count(*) FROM jobs_managed_cloud_activations),(SELECT count(*) FROM jobs_managed_cloud_head_transitions),(SELECT count(*) FROM jobs_managed_cloud_activation_requirements),(SELECT count(*) FROM jobs_managed_cloud_signature_sets)", [], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).unwrap(),
            DbPool::Postgres(_) => { let row=pool.get_pg().unwrap().query_one("SELECT (SELECT count(*) FROM jobs_managed_cloud_activations),(SELECT count(*) FROM jobs_managed_cloud_head_transitions),(SELECT count(*) FROM jobs_managed_cloud_activation_requirements),(SELECT count(*) FROM jobs_managed_cloud_signature_sets)", &[]).unwrap(); (row.get(0),row.get(1),row.get(2),row.get(3)) }
        }
            })
        };
        let before = counts();
        assert_eq!(
            (before.0, before.1, before.2),
            (0, 0, 0),
            "fixture must not seed a legacy head"
        );
        let wrong_role = legacy_tests::managed_cloud_test_signed_envelope(
            &doc,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "promotion",
            doc.issued_at_ms + 1_000,
        );
        assert!(install_provisioning_head(
            pool,
            &process,
            &InstallProvisioningRequest {
                envelope: wrong_role.envelope
            },
            "phase656-test"
        )
        .is_err());
        assert_eq!(
            counts(),
            before,
            "failed signature must roll back provisional rows"
        );
        let first = install_provisioning_head(pool, &process, &input, "phase656-test").unwrap();
        assert_eq!(first.head_revision, 1);
        assert!(!first.replayed);
        let installed = counts();
        assert_eq!((installed.0, installed.1, installed.2), (1, 1, 9));
        let replay = install_provisioning_head(pool, &process, &input, "phase656-test").unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.activation_sha256, first.activation_sha256);
        assert_eq!(replay.transition_sha256, first.transition_sha256);
        assert_eq!(counts(), installed, "exact replay must not insert rows");
        let grant = runtime::issue_provisioning_runtime_grant(
            pool,
            &process,
            &NewManagedCloudRuntimeGrant {
                issuance_ref: "phase656-grant-issuance".into(),
                scope: doc.scope.clone(),
                activation_sha256: first.activation_sha256.clone(),
                manifest_sha256: doc.manifest_sha256.clone(),
                component_id: "jobs-api".into(),
                role: "jobs_api".into(),
                expected_worker_id: "phase656-worker-instance".into(),
                authorization_ref: "phase656-authorization".into(),
                created_by: "phase656-test".into(),
                ttl_ms: 10_000,
            },
        )
        .unwrap();
        let grant_token = grant.grant_token.clone().unwrap();
        let runtime_id = "phase656-runtime-instance";
        let session =
            super::super::managed_cloud_provisioning_protocol::derive_runtime_session_token_v2(
                &grant_token,
                &grant.grant_id,
                runtime_id,
                &doc.authority_binding,
            )
            .unwrap();
        let claim = super::super::managed_cloud_provisioning_protocol::ClaimRequest {
            version: 2,
            authority_binding: doc.authority_binding.clone(),
            grant_id: grant.grant_id.clone(),
            grant_token,
            runtime_instance_id: runtime_id.into(),
            worker_id: grant.expected_worker_id.clone(),
            session_token: session,
            runtime_identity_sha256: grant.expected_runtime_identity_sha256.clone(),
        };
        let claimed = runtime::claim_provisioning_runtime_grant(pool, &process, &claim).unwrap();
        assert_eq!(claimed.activation_sha256, first.activation_sha256);
        assert!(!claimed.replayed);
        let readiness = resolve_managed_cloud_readiness(
            pool,
            &ManagedCloudReadinessQuery {
                scope: doc.scope.clone(),
                account_id: None,
            },
        )
        .unwrap();
        assert_eq!(
            readiness.status.unavailability_reason.as_deref(),
            Some("provisioned_nonserving")
        );
        assert!(!readiness.status.authority_ready && !readiness.status.customer_admission);
        assert!(managed_cloud_admission_from_readiness(&readiness).is_err());
        let inert = crate::db::run_blocking_db(|| {
            match pool {
            DbPool::Sqlite(_) => pool.get().unwrap().query_row("SELECT count(*) FROM jobs_managed_cloud_activations WHERE activation_sha256=?1 AND cloud_distribution_enabled=FALSE AND workflow_command_dispatch_enabled=FALSE AND workflow_cleanup_enabled=FALSE AND direct_discovery_enabled=FALSE AND global_discovery_enabled=FALSE AND source_verification_enabled=FALSE", [&first.activation_sha256], |row| row.get::<_,i64>(0)).unwrap(),
            DbPool::Postgres(_) => pool.get_pg().unwrap().query_one("SELECT count(*) FROM jobs_managed_cloud_activations WHERE activation_sha256=$1 AND cloud_distribution_enabled=FALSE AND workflow_command_dispatch_enabled=FALSE AND workflow_cleanup_enabled=FALSE AND direct_discovery_enabled=FALSE AND global_discovery_enabled=FALSE AND source_verification_enabled=FALSE", &[&first.activation_sha256]).unwrap().get(0),
        }
        });
        assert_eq!(inert, 1);

        let mut changed = doc.clone();
        changed.activation_id = "phase656-other-activation".into();
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &changed,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            changed.issued_at_ms + 1_000,
        );
        assert!(matches!(
            install_provisioning_head(
                pool,
                &process,
                &InstallProvisioningRequest {
                    envelope: signed.envelope
                },
                "phase656-test"
            ),
            Err(ManagedCloudRegistryError::CompareAndSwapConflict)
        ));
        let mut synthetic = changed;
        synthetic.authority_binding.purpose = Purpose::NonproductionSynthetic;
        synthetic.authority_binding.context_sha256 = Some("c".repeat(64));
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &synthetic,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            synthetic.issued_at_ms + 1_000,
        );
        assert!(matches!(
            install_provisioning_head(
                pool,
                &process,
                &InstallProvisioningRequest {
                    envelope: signed.envelope
                },
                "phase656-test"
            ),
            Err(ManagedCloudRegistryError::InvalidAuthority)
        ));

        let mut next_queue = request();
        next_queue.issuance_ref = "phase656-replacement-issuance".into();
        next_queue.task_queue = "phase656-replacement-queue".into();
        let next =
            allocate_provisioning_queue(pool, &process, &next_queue, "phase656-test").unwrap();
        let mut replacement = doc.clone();
        replacement.activation_id = "phase656-replacement-activation".into();
        replacement.activation_generation = 2;
        replacement.channel_sequence = 2;
        replacement.expected_head_revision = 1;
        replacement.expected_transition_sha256 = Some(first.transition_sha256.clone());
        replacement.predecessor_activation_sha256 = Some(first.activation_sha256.clone());
        replacement.task_queue_sha256 = next.task_queue_sha256;
        replacement.authority_binding.queue_generation_id = next.queue_generation_id;
        let now = crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            DbPool::Postgres(_) => pool
                .get_pg()
                .unwrap()
                .query_one(
                    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
                    &[],
                )
                .unwrap()
                .get(0),
        });
        replacement.expires_at_ms = now + 15_000;
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &replacement,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            replacement.issued_at_ms + 1_000,
        );
        let replacement_request = InstallProvisioningRequest {
            envelope: signed.envelope,
        };
        let mut race_queue = request();
        race_queue.issuance_ref = "phase656-race-issuance".into();
        race_queue.task_queue = "phase656-race-queue".into();
        let race_allocation =
            allocate_provisioning_queue(pool, &process, &race_queue, "phase656-test").unwrap();
        let mut contender = replacement.clone();
        contender.activation_id = "phase656-race-activation".into();
        contender.task_queue_sha256 = race_allocation.task_queue_sha256;
        contender.authority_binding.queue_generation_id = race_allocation.queue_generation_id;
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &contender,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            contender.issued_at_ms + 1_000,
        );
        let requests = [
            replacement_request,
            InstallProvisioningRequest {
                envelope: signed.envelope,
            },
        ];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = requests
            .iter()
            .cloned()
            .map(|candidate| {
                let pool = pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    install_provisioning_head(
                        &pool,
                        &ProvisioningProcessContext::production(),
                        &candidate,
                        "phase656-test",
                    )
                })
            })
            .collect();
        let mut results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(ManagedCloudRegistryError::CompareAndSwapConflict)
                ))
                .count(),
            1
        );
        let winner = results.iter().position(Result::is_ok).unwrap();
        let current = results.remove(winner).unwrap();
        let replacement_request = &requests[winner];
        assert_eq!(current.head_revision, 2);
        assert!(matches!(
            install_provisioning_head(pool, &process, &input, "phase656-test"),
            Err(ManagedCloudRegistryError::CompareAndSwapConflict)
        ));
        assert!(
            install_provisioning_head(pool, &process, replacement_request, "phase656-test")
                .unwrap()
                .replayed
        );
        let before_expiry = counts();
        assert_eq!(
            (before_expiry.0, before_expiry.1, before_expiry.2),
            (2, 2, 18),
            "losing CAS must not write partial rows"
        );
        std::thread::sleep(std::time::Duration::from_secs(17));
        assert!(matches!(
            install_provisioning_head(pool, &process, replacement_request, "phase656-test"),
            Err(ManagedCloudRegistryError::InvalidAuthority)
        ));
        assert_eq!(
            counts(),
            before_expiry,
            "expired replay must not mutate rows"
        );

        let mut third_queue = request();
        third_queue.issuance_ref = "phase656-revocation-issuance".into();
        third_queue.task_queue = "phase656-revocation-queue".into();
        let third =
            allocate_provisioning_queue(pool, &process, &third_queue, "phase656-test").unwrap();
        let mut revocable = doc;
        revocable.activation_id = "phase656-revocable-activation".into();
        revocable.activation_generation = 3;
        revocable.channel_sequence = 3;
        revocable.expected_head_revision = 2;
        revocable.expected_transition_sha256 = Some(current.transition_sha256);
        revocable.predecessor_activation_sha256 = Some(current.activation_sha256);
        revocable.task_queue_sha256 = third.task_queue_sha256;
        revocable.authority_binding.queue_generation_id = third.queue_generation_id;
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &revocable,
            "bluey-jobs-managed-cloud-provisioning-v4",
            "general_promotion",
            revocable.issued_at_ms + 1_000,
        );
        let revocable_request = InstallProvisioningRequest {
            envelope: signed.envelope,
        };
        let current =
            install_provisioning_head(pool, &process, &revocable_request, "phase656-test").unwrap();
        assert_eq!(current.head_revision, 3);
        let revocation = ManagedCloudRevocationAuthority {
            version: 1,
            audience: MANAGED_CLOUD_REVOCATION_AUDIENCE.into(),
            revocation_id: "phase656-revoke-activation".into(),
            revocation_generation: 1,
            predecessor_revocation_sha256: None,
            trust_generation: 1,
            subject_kind: "activation".into(),
            subject_id: revocable.activation_id.clone(),
            subject_sha256: current.activation_sha256.clone(),
            reason_ref: "phase656-revocation-test".into(),
            issued_at_ms: revocable.issued_at_ms,
            effective_at_ms: revocable.issued_at_ms,
        };
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &revocation,
            MANAGED_CLOUD_REVOCATION_AUDIENCE,
            "incident",
            revocable.issued_at_ms + 1_000,
        );
        import_managed_cloud_revocation(
            pool,
            &ManagedCloudRevocationImportRequest {
                envelope: signed.envelope,
            },
            "phase656-test",
        )
        .unwrap();
        let after_revocation = counts();
        assert!(matches!(
            install_provisioning_head(pool, &process, &revocable_request, "phase656-test"),
            Err(ManagedCloudRegistryError::Revoked)
        ));
        assert_eq!(
            counts(),
            after_revocation,
            "revoked replay must not mutate rows"
        );
    }

    #[test]
    fn sqlite_signed_provisioning_first_head_replay_and_conflicts() {
        let pool = super::super::tests::test_pool();
        assert_signed_first_head(&pool);
    }

    #[test]
    fn postgres_signed_provisioning_first_head_replay_and_conflicts_when_owned() {
        let Some(url) = crate::db::owned_postgres_test::owned_url("db::jobs::managed_cloud_provisioning_installation::tests::postgres_signed_provisioning_first_head_replay_and_conflicts_when_owned") else { return; };
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        let ledger_exists: bool = pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(
            !ledger_exists,
            "owned PostgreSQL case must begin with a fresh database"
        );
        crate::db::run_migrations(&pool).unwrap();
        assert_signed_first_head(&pool);
        println!("phase656_owned_signed_first_head_complete");
    }

    fn request() -> AllocateProvisioningQueueRequest {
        AllocateProvisioningQueueRequest {
            issuance_ref: "phase656-queue-issuance-unit".into(),
            scope: ManagedCloudScope {
                environment: "production".into(),
                region: "us-east-1".into(),
                channel: "general".into(),
            },
            namespace: "phase656-unit-namespace".into(),
            task_queue: "phase656-unit-queue".into(),
        }
    }

    #[test]
    fn sqlite_queue_allocation_replays_exactly_and_refuses_recorded_reuse() {
        let pool = super::super::tests::test_pool();
        let process = ProvisioningProcessContext::production();
        let request = request();
        let first =
            allocate_provisioning_queue(&pool, &process, &request, "phase656-unit").unwrap();
        let replay =
            allocate_provisioning_queue(&pool, &process, &request, "phase656-unit").unwrap();
        assert!(!first.replayed && replay.replayed);
        assert_eq!(first.queue_generation_id, replay.queue_generation_id);
        assert_eq!(first.created_at_ms, replay.created_at_ms);
        assert_eq!(first.task_queue_sha256, replay.task_queue_sha256);
        let mut changed = request.clone();
        changed.task_queue.push_str("-other");
        assert!(matches!(
            allocate_provisioning_queue(&pool, &process, &changed, "phase656-unit"),
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
        changed = request.clone();
        changed.issuance_ref = "phase656-other-issuance-unit".into();
        assert!(matches!(
            allocate_provisioning_queue(&pool, &process, &changed, "phase656-unit"),
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
        changed = request.clone();
        changed.scope.region = "us-west-2".into();
        assert!(matches!(
            allocate_provisioning_queue(&pool, &process, &changed, "phase656-unit"),
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
        let mut invalid = request;
        invalid.namespace = "invalid\nnamespace".into();
        assert!(matches!(
            allocate_provisioning_queue(&pool, &process, &invalid, "phase656-unit"),
            Err(ManagedCloudRegistryError::InvalidRequest)
        ));
        invalid.namespace = "e\u{301}".into();
        assert!(matches!(
            allocate_provisioning_queue(&pool, &process, &invalid, "phase656-unit"),
            Err(ManagedCloudRegistryError::InvalidRequest)
        ));
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_managed_cloud_queue_generations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn sqlite_concurrent_same_issuance_creates_one_immutable_allocation() {
        let pool = super::super::tests::test_pool();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    allocate_provisioning_queue(
                        &pool,
                        &ProvisioningProcessContext::production(),
                        &request(),
                        "phase656-unit",
                    )
                    .unwrap()
                })
            })
            .collect();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(
            results[0].queue_generation_id,
            results[1].queue_generation_id
        );
        assert_eq!(results.iter().filter(|value| value.replayed).count(), 1);
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_managed_cloud_queue_generations",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }
}
