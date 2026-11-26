//! Ordinary v4 control-plane transactions. This module grants no business effect.
//! Stored allocation metadata is not physical queue/resource qualification.

use super::managed_cloud_provisioning_persistence::{StoredAuthority, PROVISIONING_AUDIENCE};
use super::managed_cloud_provisioning_protocol as wire;
use super::*;
use base64::Engine;
use postgres::types::{ToSql, Type};
use rusqlite::types::{ToSqlOutput, Value, ValueRef};
use rusqlite::{OptionalExtension, TransactionBehavior};

/// Constructed once by server/router startup, never from an HTTP purpose field.
/// No synthetic constructor exists until the physical process-context reader exists.
pub(crate) struct ProvisioningProcessContext {
    _private: (),
}

impl ProvisioningProcessContext {
    pub(crate) fn production() -> Self {
        Self { _private: () }
    }

    pub(crate) fn from_environment() -> ManagedCloudResult<Self> {
        Self::from_startup_value(std::env::var_os("BLUEY_JOBS_PROVISIONING_PROCESS_CONTEXT"))
    }

    fn from_startup_value(value: Option<std::ffi::OsString>) -> ManagedCloudResult<Self> {
        match value {
            Some(value) if value.to_str() == Some("production") => Ok(Self::production()),
            None => Ok(Self::production()),
            _ => Err(ManagedCloudRegistryError::InvalidAuthority),
        }
    }

    pub(super) fn permits(&self, binding: &wire::AuthorityBinding) -> ManagedCloudResult<()> {
        ensure(binding.purpose == wire::Purpose::Production && binding.context_sha256.is_none())
    }
}

fn ensure(valid: bool) -> ManagedCloudResult<()> {
    if valid {
        Ok(())
    } else {
        Err(ManagedCloudRegistryError::InvalidAuthority)
    }
}

// A narrow transaction adapter keeps the five writers on the SAME lock order
// and SQL parameter bindings across the two ordinary database backends. SQL and
// table names are internal constants, never caller supplied. No nested wrapper.
enum Argument {
    Text(String),
    Integer(i64),
    Null(Option<String>),
    NullInteger(Option<i64>),
}
impl From<&str> for Argument {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<&String> for Argument {
    fn from(v: &String) -> Self {
        Self::Text(v.clone())
    }
}
impl From<String> for Argument {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<i64> for Argument {
    fn from(v: i64) -> Self {
        Self::Integer(v)
    }
}
impl From<Option<&str>> for Argument {
    fn from(v: Option<&str>) -> Self {
        v.map(Self::from).unwrap_or(Self::Null(None))
    }
}
impl From<Option<i64>> for Argument {
    fn from(v: Option<i64>) -> Self {
        v.map(Self::Integer).unwrap_or(Self::NullInteger(None))
    }
}
impl Argument {
    fn postgres(&self) -> &(dyn ToSql + Sync) {
        match self {
            Self::Text(v) => v,
            Self::Integer(v) => v,
            Self::Null(v) => v,
            Self::NullInteger(v) => v,
        }
    }
}
impl rusqlite::ToSql for Argument {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Self::Text(v) => ValueRef::Text(v.as_bytes()),
            Self::Integer(v) => ValueRef::Integer(*v),
            Self::Null(_) | Self::NullInteger(_) => ValueRef::Null,
        }))
    }
}
macro_rules! args { ($($value:expr),* $(,)?) => { [$(Argument::from($value)),*] }; }

struct Record(Vec<Value>);
impl Record {
    fn text(&self, index: usize) -> ManagedCloudResult<String> {
        match self.0.get(index) {
            Some(Value::Text(v)) => Ok(v.clone()),
            _ => Err(ManagedCloudRegistryError::InvalidAuthority),
        }
    }
    fn integer(&self, index: usize) -> ManagedCloudResult<i64> {
        match self.0.get(index) {
            Some(Value::Integer(v)) => Ok(*v),
            _ => Err(ManagedCloudRegistryError::InvalidAuthority),
        }
    }
    fn optional_text(&self, index: usize) -> ManagedCloudResult<Option<String>> {
        match self.0.get(index) {
            Some(Value::Null) => Ok(None),
            _ => self.text(index).map(Some),
        }
    }
    fn optional_integer(&self, index: usize) -> ManagedCloudResult<Option<i64>> {
        match self.0.get(index) {
            Some(Value::Null) => Ok(None),
            _ => self.integer(index).map(Some),
        }
    }
}

fn sqlite_bind(
    statement: &mut rusqlite::Statement<'_>,
    values: &[Argument],
) -> ManagedCloudResult<()> {
    // SQLite assigns *encounter* positions to named $N parameters, whereas PG
    // uses numeric positions. Bind by exact name: UPDATE's SET $3 precedes $1.
    ensure(statement.parameter_count() == values.len())?;
    for (index, value) in values.iter().enumerate() {
        let position = statement
            .parameter_index(&format!("${}", index + 1))
            .map_err(managed_cloud_storage)?
            .ok_or(ManagedCloudRegistryError::InvalidRequest)?;
        statement
            .raw_bind_parameter(position, value)
            .map_err(managed_cloud_storage)?;
    }
    Ok(())
}

pub(super) enum ControlTransaction<'a> {
    Sqlite(rusqlite::Transaction<'a>),
    Postgres(postgres::Transaction<'a>),
}
impl ControlTransaction<'_> {
    fn rows(&mut self, sql: &str, values: &[Argument]) -> ManagedCloudResult<Vec<Record>> {
        let mut output = Vec::new();
        match self {
            Self::Sqlite(tx) => {
                let mut statement = tx.prepare(sql).map_err(managed_cloud_storage)?;
                let count = statement.column_count();
                sqlite_bind(&mut statement, values)?;
                let mut rows = statement.raw_query();
                while let Some(row) = rows.next().map_err(managed_cloud_storage)? {
                    ensure(output.len() < 1024)?;
                    let values = (0..count)
                        .map(|i| row.get(i))
                        .collect::<rusqlite::Result<Vec<Value>>>()
                        .map_err(managed_cloud_storage)?;
                    output.push(Record(values));
                }
            }
            Self::Postgres(tx) => {
                let params: Vec<_> = values.iter().map(Argument::postgres).collect();
                let mut rows = tx.query_raw(sql, params).map_err(managed_cloud_storage)?;
                use postgres::fallible_iterator::FallibleIterator;
                while let Some(row) = rows.next().map_err(managed_cloud_storage)? {
                    ensure(output.len() < 1024)?;
                    let mut values = Vec::with_capacity(row.len());
                    for (i, column) in row.columns().iter().enumerate() {
                        let value = match *column.type_() {
                            Type::INT8 => row
                                .try_get::<_, Option<i64>>(i)
                                .map(|v| v.map(Value::Integer)),
                            Type::INT4 => row
                                .try_get::<_, Option<i32>>(i)
                                .map(|v| v.map(|v| Value::Integer(i64::from(v)))),
                            Type::BOOL => row
                                .try_get::<_, Option<bool>>(i)
                                .map(|v| v.map(|v| Value::Integer(i64::from(v)))),
                            _ => row
                                .try_get::<_, Option<String>>(i)
                                .map(|v| v.map(Value::Text)),
                        }
                        .map_err(managed_cloud_storage)?
                        .unwrap_or(Value::Null);
                        values.push(value);
                    }
                    output.push(Record(values));
                }
            }
        }
        Ok(output)
    }

    fn optional(&mut self, sql: &str, values: &[Argument]) -> ManagedCloudResult<Option<Record>> {
        let mut rows = self.rows(sql, values)?;
        ensure(rows.len() <= 1)?;
        Ok(rows.pop())
    }
    fn one(&mut self, sql: &str, values: &[Argument]) -> ManagedCloudResult<Record> {
        self.optional(sql, values)?
            .ok_or(ManagedCloudRegistryError::NotFound)
    }
    fn execute(&mut self, sql: &str, values: &[Argument]) -> ManagedCloudResult<u64> {
        match self {
            Self::Sqlite(tx) => {
                let mut statement = tx.prepare(sql).map_err(managed_cloud_storage)?;
                sqlite_bind(&mut statement, values)?;
                statement
                    .raw_execute()
                    .map(|v| v as u64)
                    .map_err(managed_cloud_storage)
            }
            Self::Postgres(tx) => tx
                .execute(
                    sql,
                    &values.iter().map(Argument::postgres).collect::<Vec<_>>(),
                )
                .map_err(managed_cloud_storage),
        }
    }
    pub(super) fn now(&mut self) -> ManagedCloudResult<i64> {
        match self {
            Self::Sqlite(tx) => managed_cloud_db_now_sqlite(tx),
            Self::Postgres(tx) => managed_cloud_db_now_postgres(tx),
        }
    }
    fn head(&mut self, scope: &ManagedCloudScope) -> ManagedCloudResult<ManagedCloudResolvedHead> {
        match self {
            Self::Sqlite(tx) => sqlite_managed_cloud_resolved_head(tx, scope),
            Self::Postgres(tx) => {
                lock_managed_cloud_release_registry_postgres_tx(tx, scope)?;
                postgres_managed_cloud_resolved_head(tx, scope)
            }
        }?
        .ok_or(ManagedCloudRegistryError::Unavailable)
    }
    fn commit(self) -> ManagedCloudResult<()> {
        match self {
            Self::Sqlite(tx) => tx.commit().map_err(managed_cloud_storage),
            Self::Postgres(tx) => tx.commit().map_err(managed_cloud_storage),
        }
    }
}

pub(super) fn transaction<T>(
    pool: &DbPool,
    apply: impl FnOnce(&mut ControlTransaction<'_>) -> ManagedCloudResult<T>,
) -> ManagedCloudResult<T> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let mut control = ControlTransaction::Sqlite(tx);
            let result = apply(&mut control)?;
            control.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            // All writers, including replay, take H -> M before grant/instance/boot.
            lock_operational_hold_shared_postgres_tx(&mut tx)
                .map_err(map_managed_cloud_operational_error)?;
            lock_managed_cloud_release_registry_exclusive_postgres_tx(&mut tx)?;
            let mut control = ControlTransaction::Postgres(tx);
            let result = apply(&mut control)?;
            control.commit()?;
            Ok(result)
        }
    })
}

struct ProvisioningControlAuthority {
    head: ManagedCloudResolvedHead,
    binding: wire::AuthorityBinding,
    heartbeat_ttl_ms: i64,
}

fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(
    encoded: &str,
) -> ManagedCloudResult<T> {
    managed_cloud_parse_canonical(&managed_cloud_decode_base64url(encoded)?)
}
fn encode<T: serde::Serialize>(value: &T) -> ManagedCloudResult<String> {
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(managed_cloud_canonical_json(value)?))
}
fn json<T: serde::Serialize>(value: &T) -> ManagedCloudResult<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| ManagedCloudRegistryError::InvalidRequest)
}

fn authority(
    tx: &mut ControlTransaction<'_>,
    process: &ProvisioningProcessContext,
    scope: &ManagedCloudScope,
    now: i64,
) -> ManagedCloudResult<ProvisioningControlAuthority> {
    let head = tx.head(scope)?;
    let StoredAuthority::ProvisioningV4(document) = &head.authority else {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    };
    let binding = document.control_binding().clone();
    process.permits(&binding)?;
    ensure(document.control_not_before_ms() <= now && head.activation_expires_at_ms > now)?;
    let ttl = document.control_ttl_ms();
    let namespace_sha256 = document.control_namespace_sha256().to_owned();
    let suffix = if matches!(tx, ControlTransaction::Postgres(_)) {
        " FOR UPDATE"
    } else {
        ""
    };
    let queue = tx.one(
        &format!(
            "SELECT namespace,task_queue FROM jobs_managed_cloud_queue_generations
        WHERE queue_generation_id=$1 AND environment=$2 AND region=$3 AND channel=$4
          AND purpose='production' AND task_queue_sha256=$5{suffix}"
        ),
        &args![
            &binding.queue_generation_id,
            &scope.environment,
            &scope.region,
            &scope.channel,
            &head.task_queue_sha256
        ],
    )?;
    let namespace = queue.text(0)?;
    let task_queue = queue.text(1)?;
    ensure(
        managed_cloud_task_queue_sha256(&namespace, &task_queue)? == head.task_queue_sha256
            && managed_cloud_sha256(namespace.as_bytes()) == namespace_sha256,
    )?;
    let policy = match tx {
        ControlTransaction::Sqlite(tx) => {
            sqlite_managed_cloud_policy_by_generation(tx, head.trust_generation)?
        }
        ControlTransaction::Postgres(tx) => {
            postgres_managed_cloud_policy_by_generation(tx, head.trust_generation)?
        }
    };
    let promotion = if scope.channel == "general" {
        "general_promotion"
    } else {
        "promotion"
    };
    // Every target is reverified, not inferred from a foreign key or hash shape.
    let targets = tx.one("SELECT a.canonical_activation_base64url,a.authorization_signature_set_sha256,
        m.canonical_manifest_base64url,m.authorization_signature_set_sha256,
        c.canonical_cohort_base64url,c.authorization_signature_set_sha256,
        c.not_before_ms,c.expires_at_ms
        FROM jobs_managed_cloud_activations a
        JOIN jobs_managed_cloud_manifests m ON m.manifest_sha256=a.manifest_sha256 AND m.trust_generation=a.trust_generation
        JOIN jobs_managed_cloud_cohorts c ON c.cohort_sha256=a.cohort_sha256 AND c.trust_generation=a.trust_generation
        WHERE a.activation_sha256=$1 AND a.manifest_sha256=$2 AND a.cohort_sha256=$3",
        &args![&head.activation_sha256, &head.manifest_sha256, &head.cohort_sha256])?;
    ensure(targets.integer(6)? <= now && targets.integer(7)? > now)?;
    let release: ManagedCloudReleaseAuthority = decode(&targets.text(2)?)?;
    validate_managed_cloud_release(&release)?;
    let cohort: ManagedCloudCohortAuthority = decode(&targets.text(4)?)?;
    validate_managed_cloud_cohort(&cohort)?;
    ensure(
        cohort.scope == *scope
            && cohort.trust_generation == head.trust_generation
            && release.feature_authority_sha256
                == managed_cloud_digest(&release.feature_authority)?,
    )?;
    for (canonical_index, signature_index, audience, role, issued, expected) in [
        (
            0,
            1,
            PROVISIONING_AUDIENCE,
            promotion,
            document.control_issued_at_ms(),
            head.activation_sha256.as_str(),
        ),
        (
            2,
            3,
            release.audience.as_str(),
            "release",
            release.published_at_ms,
            head.manifest_sha256.as_str(),
        ),
        (
            4,
            5,
            MANAGED_CLOUD_COHORT_AUDIENCE,
            promotion,
            cohort.issued_at_ms,
            head.cohort_sha256.as_str(),
        ),
    ] {
        let bytes = managed_cloud_decode_base64url(&targets.text(canonical_index)?)?;
        ensure(managed_cloud_sha256(&bytes) == expected)?;
        let signature_hash = targets.text(signature_index)?;
        let stored = tx.one("SELECT canonical_signature_set_base64url FROM jobs_managed_cloud_signature_sets
            WHERE signature_set_sha256=$1 AND target_audience=$2 AND target_sha256=$3 AND trust_generation=$4 AND role=$5",
            &args![&signature_hash, audience, expected, head.trust_generation, role])?;
        let signature_bytes = managed_cloud_decode_base64url(&stored.text(0)?)?;
        ensure(managed_cloud_sha256(&signature_bytes) == signature_hash)?;
        let signatures: ManagedCloudSignatureSetAuthority =
            managed_cloud_parse_canonical(&signature_bytes)?;
        verify_managed_cloud_signature_set(
            &bytes,
            &signatures,
            &policy.policy,
            role,
            audience,
            issued,
            now,
        )?;
        match tx {
            ControlTransaction::Sqlite(tx) => {
                require_sqlite_managed_cloud_signing_authority_not_revoked(
                    tx,
                    &policy,
                    &signatures,
                    now,
                )?
            }
            ControlTransaction::Postgres(tx) => {
                require_postgres_managed_cloud_signing_authority_not_revoked(
                    tx,
                    &policy,
                    &signatures,
                    now,
                )?
            }
        }
    }
    if tx.optional("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1 AND (
        (subject_kind='activation' AND subject_sha256=$2) OR (subject_kind IN ('manifest','release') AND subject_sha256=$3)
        OR (subject_kind='cohort' AND subject_sha256=$4)) LIMIT 1",
        &args![now, &head.activation_sha256, &head.manifest_sha256, &head.cohort_sha256])?.is_some() {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    Ok(ProvisioningControlAuthority {
        head,
        binding,
        heartbeat_ttl_ms: ttl,
    })
}

/// Recheck the installed head and all signed control-plane dependencies in the
/// caller's transaction. This does not grant business or serving authority.
pub(super) fn validate_installed_provisioning_authority(
    tx: &mut ControlTransaction<'_>,
    process: &ProvisioningProcessContext,
    scope: &ManagedCloudScope,
    now: i64,
) -> ManagedCloudResult<()> {
    authority(tx, process, scope, now).map(|_| ())
}

fn observation_table(role: &str) -> &'static str {
    if role == "original_source_verifier" {
        "jobs_managed_cloud_osv_runtime_observations"
    } else {
        "jobs_managed_cloud_runtime_observations"
    }
}

impl ControlTransaction<'_> {
    fn grant(
        &mut self,
        id: &str,
    ) -> ManagedCloudResult<(ManagedCloudRuntimeGrant, ManagedCloudRuntimeAuthorityTables)> {
        match self {
            Self::Sqlite(tx) => sqlite_managed_cloud_runtime_grant_by_id_any(tx, id),
            Self::Postgres(tx) => postgres_managed_cloud_runtime_grant_by_id_any(tx, id),
        }?
        .ok_or(ManagedCloudRegistryError::NotFound)
    }
    fn grant_by_reference(
        &mut self,
        reference: &str,
    ) -> ManagedCloudResult<Option<(ManagedCloudRuntimeGrant, ManagedCloudRuntimeAuthorityTables)>>
    {
        match self {
            Self::Sqlite(tx) => {
                sqlite_managed_cloud_runtime_grant_by_issuance_ref_any(tx, reference)
            }
            Self::Postgres(tx) => {
                postgres_managed_cloud_runtime_grant_by_issuance_ref_any(tx, reference)
            }
        }
    }
    fn instance(
        &mut self,
        id: &str,
    ) -> ManagedCloudResult<(
        ManagedCloudRuntimeInstance,
        String,
        ManagedCloudRuntimeAuthorityTables,
    )> {
        let mut found = None;
        for tables in [
            MANAGED_CLOUD_BASE_RUNTIME_TABLES,
            MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
        ] {
            let value = match self {
                Self::Sqlite(tx) => tx.query_row(&format!("SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS} FROM {} WHERE runtime_instance_id=?1", tables.instances),
                    params![id], managed_cloud_runtime_instance_from_sqlite).optional().map_err(managed_cloud_storage)?,
                Self::Postgres(tx) => tx.query_opt(&format!("SELECT {MANAGED_CLOUD_RUNTIME_INSTANCE_COLUMNS} FROM {} WHERE runtime_instance_id=$1 FOR UPDATE", tables.instances),
                    &[&id]).map_err(managed_cloud_storage)?.map(|row| managed_cloud_runtime_instance_from_postgres(&row)),
            };
            if let Some((instance, proof)) = value {
                ensure(found.is_none())?;
                let protocol = self.one(
                    &format!(
                        "SELECT protocol_version FROM {} WHERE runtime_instance_id=$1",
                        tables.instances
                    ),
                    &args![id],
                )?;
                ensure(protocol.integer(0)? == 2)?;
                found = Some((instance, proof, tables));
            }
        }
        found.ok_or(ManagedCloudRegistryError::NotFound)
    }
}

fn grant_authority(
    tx: &mut ControlTransaction<'_>,
    authority: &ProvisioningControlAuthority,
    input: &NewManagedCloudRuntimeGrant,
) -> ManagedCloudResult<ManagedCloudGrantAuthority> {
    let h = &authority.head;
    ensure(
        h.activation_sha256 == input.activation_sha256
            && h.manifest_sha256 == input.manifest_sha256,
    )?;
    let identity_table = if input.role == "original_source_verifier" {
        "jobs_managed_cloud_manifest_original_source_verifier_identities"
    } else {
        "jobs_managed_cloud_manifest_runtime_identities"
    };
    let row = tx.one(&format!("SELECT c.artifact_sha256,m.config_schema_sha256,m.migration_set_sha256,
        m.protocol_set_sha256,r.dependency_evidence_sha256,i.runtime_identity_sha256
        FROM jobs_managed_cloud_manifest_components c
        JOIN jobs_managed_cloud_manifests m ON m.manifest_sha256=c.manifest_sha256
        JOIN jobs_managed_cloud_manifest_capabilities k ON k.manifest_sha256=c.manifest_sha256 AND k.component_id=c.component_id
        JOIN jobs_managed_cloud_activation_requirements r ON r.activation_sha256=$1 AND r.role=k.capability
        JOIN {identity_table} i ON i.manifest_sha256=c.manifest_sha256 AND i.component_id=c.component_id AND i.role=k.capability
        WHERE c.manifest_sha256=$2 AND c.component_id=$3 AND k.capability=$4"),
        &args![&h.activation_sha256, &h.manifest_sha256, &input.component_id, &input.role])?;
    let artifact = row.text(0)?;
    let dependency = wire::dependency_evidence_sha256_v2(
        &wire::RuntimeDependencyBinding {
            role: &input.role,
            activation_sha256: &h.activation_sha256,
            manifest_sha256: &h.manifest_sha256,
            component_id: &input.component_id,
            artifact_sha256: &artifact,
            task_queue_sha256: &h.task_queue_sha256,
            failure_converter_sha256: &h.failure_converter_sha256,
        },
        &authority.binding,
    )?;
    ensure(row.text(4)? == dependency)?;
    Ok(ManagedCloudGrantAuthority {
        head_revision: h.head_revision,
        transition_sha256: h.transition_sha256.clone(),
        artifact_sha256: artifact,
        config_schema_sha256: row.text(1)?,
        migration_set_sha256: row.text(2)?,
        protocol_set_sha256: row.text(3)?,
        task_queue_sha256: h.task_queue_sha256.clone(),
        failure_converter_sha256: h.failure_converter_sha256.clone(),
        dependency_evidence_sha256: dependency,
        runtime_identity_sha256: row.text(5)?,
        activation_expires_at_ms: h.activation_expires_at_ms,
    })
}

fn require_grant_active(
    tx: &mut ControlTransaction<'_>,
    authority: &ProvisioningControlAuthority,
    grant: &ManagedCloudRuntimeGrant,
    tables: ManagedCloudRuntimeAuthorityTables,
    now: i64,
) -> ManagedCloudResult<()> {
    if grant.expires_at_ms <= now {
        return Err(ManagedCloudRegistryError::GrantExpired);
    }
    let derived = grant_authority(
        tx,
        authority,
        &NewManagedCloudRuntimeGrant {
            issuance_ref: grant.issuance_ref.clone(),
            scope: grant.scope.clone(),
            activation_sha256: grant.activation_sha256.clone(),
            manifest_sha256: grant.manifest_sha256.clone(),
            component_id: grant.component_id.clone(),
            role: grant.role.clone(),
            expected_worker_id: grant.expected_worker_id.clone(),
            authorization_ref: grant.authorization_ref.clone(),
            created_by: grant.created_by.clone(),
            ttl_ms: grant.expires_at_ms - grant.created_at_ms,
        },
    )?;
    ensure(
        grant.head_revision == derived.head_revision
            && grant.transition_sha256 == derived.transition_sha256
            && grant.artifact_sha256 == derived.artifact_sha256
            && grant.config_schema_sha256 == derived.config_schema_sha256
            && grant.migration_set_sha256 == derived.migration_set_sha256
            && grant.protocol_set_sha256 == derived.protocol_set_sha256
            && grant.task_queue_sha256 == derived.task_queue_sha256
            && grant.failure_converter_sha256 == derived.failure_converter_sha256
            && grant.dependency_evidence_sha256 == derived.dependency_evidence_sha256
            && grant.expected_runtime_identity_sha256 == derived.runtime_identity_sha256
            && grant.activation_expires_at_ms == derived.activation_expires_at_ms,
    )?;
    if tx
        .optional(
            &format!(
                "SELECT 1 FROM {} WHERE grant_id=$1",
                tables.grant_revocations
            ),
            &args![&grant.grant_id],
        )?
        .is_some()
        || tx
            .optional(
                "SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1 AND (
            (subject_kind='runtime_grant' AND subject_id=$2 AND subject_sha256=$3)
            OR (subject_kind='component' AND subject_id=$4 AND subject_sha256=$5)) LIMIT 1",
                &args![
                    now,
                    &grant.grant_id,
                    &grant.token_sha256,
                    &grant.component_id,
                    &grant.artifact_sha256
                ],
            )?
            .is_some()
    {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    Ok(())
}

fn current_latch(
    tx: &mut ControlTransaction<'_>,
    authority: &ProvisioningControlAuthority,
) -> ManagedCloudResult<Option<String>> {
    let head = &authority.head;
    let row = tx.optional(
        "SELECT latch_sha256 FROM jobs_managed_cloud_serving_latches
        WHERE activation_sha256=$1 AND head_revision=$2 AND transition_sha256=$3",
        &args![
            &head.activation_sha256,
            head.head_revision,
            &head.transition_sha256
        ],
    )?;
    // Deliberately do not treat metadata + a signature-target FK as a validated
    // typed signed latch. The signed-latch document/reader is a separate HOLD.
    // No row is an ordinary, usable nonserving startup, not Unavailable.
    ensure(row.is_none())?;
    Ok(None)
}

fn claim_response(
    instance: ManagedCloudRuntimeInstance,
    binding: wire::AuthorityBinding,
    latch: Option<String>,
) -> wire::ClaimResponse {
    wire::ClaimResponse {
        version: 2,
        authority_binding: binding,
        serving_latch_sha256: latch,
        grant_id: instance.grant_id,
        runtime_instance_id: instance.runtime_instance_id,
        runtime_identity_sha256: instance.runtime_identity_sha256,
        worker_id: instance.worker_id,
        scope: instance.scope,
        activation_sha256: instance.activation_sha256,
        activation_expires_at_ms: instance.activation_expires_at_ms,
        manifest_sha256: instance.manifest_sha256,
        component_id: instance.component_id,
        role: instance.role,
        head_revision: instance.head_revision,
        transition_sha256: instance.transition_sha256,
        artifact_sha256: instance.artifact_sha256,
        config_schema_sha256: instance.config_schema_sha256,
        migration_set_sha256: instance.migration_set_sha256,
        protocol_set_sha256: instance.protocol_set_sha256,
        task_queue_sha256: instance.task_queue_sha256,
        failure_converter_sha256: instance.failure_converter_sha256,
        dependency_evidence_sha256: instance.dependency_evidence_sha256,
        instance_epoch: instance.instance_epoch,
        next_heartbeat_sequence: instance.next_heartbeat_sequence,
        claimed_at_ms: instance.claimed_at_ms,
        replayed: instance.replayed,
    }
}

pub(crate) fn claim_provisioning_runtime_grant(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    input: &wire::ClaimRequest,
) -> ManagedCloudResult<wire::ClaimResponse> {
    let input = wire::parse_claim_request(&json(input)?)?;
    process.permits(&input.authority_binding)?;
    transaction(pool, |tx| {
        let mut scope = None;
        for family in [
            MANAGED_CLOUD_BASE_RUNTIME_TABLES,
            MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
        ] {
            if let Some(row) = tx.optional(
                &format!(
                    "SELECT environment,region,channel FROM {} WHERE grant_id=$1",
                    family.grants
                ),
                &args![&input.grant_id],
            )? {
                ensure(scope.is_none())?;
                scope = Some(ManagedCloudScope {
                    environment: row.text(0)?,
                    region: row.text(1)?,
                    channel: row.text(2)?,
                });
            }
        }
        let now = tx.now()?;
        let current = authority(
            tx,
            process,
            &scope.ok_or(ManagedCloudRegistryError::NotFound)?,
            now,
        )?;
        let (grant, tables) = tx.grant(&input.grant_id)?;
        require_managed_cloud_secret_match(
            &grant.token_sha256,
            &managed_cloud_token_sha256(&input.grant_token),
        )?;
        ensure(
            grant.expected_worker_id == input.worker_id
                && grant.expected_runtime_identity_sha256 == input.runtime_identity_sha256,
        )?;
        let expected_session = wire::derive_runtime_session_token_v2(
            &input.grant_token,
            &input.grant_id,
            &input.runtime_instance_id,
            &input.authority_binding,
        )?;
        require_managed_cloud_secret_match(&expected_session, &input.session_token)?;
        ensure(current.binding == input.authority_binding && current.head.scope == grant.scope)?;
        require_grant_active(tx, &current, &grant, tables, now)?;
        if let Some(row) = tx.optional(
            &format!(
                "SELECT runtime_instance_id FROM {} WHERE grant_id=$1",
                tables.instances
            ),
            &args![&input.grant_id],
        )? {
            let (mut instance, proof, stored_tables) = tx.instance(&row.text(0)?)?;
            ensure(
                stored_tables == tables
                    && instance.runtime_instance_id == input.runtime_instance_id
                    && instance.worker_id == input.worker_id
                    && instance.runtime_identity_sha256 == input.runtime_identity_sha256,
            )?;
            let actual = managed_cloud_runtime_session_proof_hmac(
                &input.session_token,
                &input.grant_id,
                &input.worker_id,
                &input.runtime_instance_id,
                instance.instance_epoch,
            )?;
            require_managed_cloud_secret_match(&proof, &actual)?;
            let stored = tx.one(
                &format!(
                    "SELECT claimed_latch_sha256 FROM {} WHERE runtime_instance_id=$1",
                    tables.instances
                ),
                &args![&input.runtime_instance_id],
            )?;
            let cursor = tx.optional(&format!("SELECT heartbeat_sequence FROM {} WHERE runtime_instance_id=$1 AND instance_epoch=$2", tables.heartbeats),
                &args![&input.runtime_instance_id, instance.instance_epoch])?;
            instance.next_heartbeat_sequence = cursor
                .map(|r| r.integer(0))
                .transpose()?
                .unwrap_or(0)
                .checked_add(1)
                .filter(|v| managed_cloud_safe_integer(*v, true))
                .ok_or(ManagedCloudRegistryError::HeartbeatSequenceConflict)?;
            instance.replayed = true;
            return Ok(claim_response(
                instance,
                current.binding,
                stored.optional_text(0)?,
            ));
        }
        for family in [
            MANAGED_CLOUD_BASE_RUNTIME_TABLES,
            MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
        ] {
            if tx
                .optional(
                    &format!(
                        "SELECT 1 FROM {} WHERE runtime_instance_id=$1",
                        family.instances
                    ),
                    &args![&input.runtime_instance_id],
                )?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
        }
        let latch = current_latch(tx, &current)?;
        let proof = managed_cloud_runtime_session_proof_hmac(
            &input.session_token,
            &input.grant_id,
            &input.worker_id,
            &input.runtime_instance_id,
            1,
        )?;
        tx.execute(&format!("INSERT INTO {}(grant_id,runtime_instance_id,runtime_identity_sha256,worker_id,
            session_proof_hmac_sha256,environment,region,channel,activation_sha256,manifest_sha256,component_id,role,
            head_revision,transition_sha256,artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,
            task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,activation_expires_at_ms,
            instance_epoch,claimed_at_ms,protocol_version,claimed_latch_sha256)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,1,$23,2,$24)", tables.instances),
            &args![&grant.grant_id, &input.runtime_instance_id, &input.runtime_identity_sha256, &input.worker_id, proof,
                &grant.scope.environment, &grant.scope.region, &grant.scope.channel, &grant.activation_sha256,
                &grant.manifest_sha256, &grant.component_id, &grant.role, grant.head_revision, &grant.transition_sha256,
                &grant.artifact_sha256, &grant.config_schema_sha256, &grant.migration_set_sha256, &grant.protocol_set_sha256,
                &grant.task_queue_sha256, &grant.failure_converter_sha256, &grant.dependency_evidence_sha256,
                grant.activation_expires_at_ms, now, latch.as_deref()])?;
        tx.execute(
            &format!(
                "UPDATE {} SET grant_token_ciphertext=NULL WHERE grant_id=$1",
                tables.grants
            ),
            &args![&grant.grant_id],
        )?;
        let (instance, _, _) = tx.instance(&input.runtime_instance_id)?;
        Ok(claim_response(instance, current.binding, latch))
    })
}

struct AuthenticatedInstance {
    instance: ManagedCloudRuntimeInstance,
    tables: ManagedCloudRuntimeAuthorityTables,
    authority: ProvisioningControlAuthority,
}

fn authenticated_instance(
    tx: &mut ControlTransaction<'_>,
    process: &ProvisioningProcessContext,
    runtime_id: &str,
    worker_id: &str,
    session: &str,
    binding: &wire::AuthorityBinding,
    now: i64,
) -> ManagedCloudResult<AuthenticatedInstance> {
    process.permits(binding)?;
    let mut located_scope = None;
    for tables in [
        MANAGED_CLOUD_BASE_RUNTIME_TABLES,
        MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
    ] {
        if let Some(row) = tx.optional(
            &format!(
                "SELECT environment,region,channel FROM {} WHERE runtime_instance_id=$1",
                tables.instances
            ),
            &args![runtime_id],
        )? {
            ensure(located_scope.is_none())?;
            located_scope = Some(ManagedCloudScope {
                environment: row.text(0)?,
                region: row.text(1)?,
                channel: row.text(2)?,
            });
        }
    }
    let scope = located_scope.ok_or(ManagedCloudRegistryError::NotFound)?;
    // Locate without a row lock, then H/M -> scope/generation -> instance. All
    // located facts are revalidated after the ordered locks, not trusted inputs.
    let authority = authority(tx, process, &scope, now)?;
    let (instance, proof, tables) = tx.instance(runtime_id)?;
    ensure(instance.worker_id == worker_id && instance.scope == scope)?;
    let expected = managed_cloud_runtime_session_proof_hmac(
        session,
        &instance.grant_id,
        worker_id,
        runtime_id,
        instance.instance_epoch,
    )?;
    require_managed_cloud_secret_match(&proof, &expected)?;
    ensure(authority.binding == *binding)?;
    let (grant, grant_tables) = tx.grant(&instance.grant_id)?;
    ensure(grant_tables == tables)?;
    require_grant_active(tx, &authority, &grant, tables, now)?;
    if tx
        .optional(
            "SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1
        AND subject_kind='runtime_instance' AND subject_id=$2 AND subject_sha256=$3 LIMIT 1",
            &args![
                now,
                &instance.runtime_instance_id,
                &instance.runtime_identity_sha256
            ],
        )?
        .is_some()
    {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    ensure(
        instance.head_revision == authority.head.head_revision
            && instance.transition_sha256 == authority.head.transition_sha256
            && instance.activation_sha256 == authority.head.activation_sha256
            && instance.manifest_sha256 == authority.head.manifest_sha256,
    )?;
    Ok(AuthenticatedInstance {
        instance,
        tables,
        authority,
    })
}

fn require_heartbeat_identity(
    instance: &ManagedCloudRuntimeInstance,
    request: &wire::HeartbeatRequest,
) -> ManagedCloudResult<()> {
    ensure(
        instance.runtime_instance_id == request.runtime_instance_id
            && instance.worker_id == request.worker_id
            && instance.head_revision == request.observed_head_revision
            && instance.transition_sha256 == request.observed_transition_sha256
            && instance.activation_sha256 == request.activation_sha256
            && instance.manifest_sha256 == request.manifest_sha256
            && instance.component_id == request.component_id
            && instance.role == request.role
            && instance.artifact_sha256 == request.artifact_sha256
            && instance.config_schema_sha256 == request.config_schema_sha256
            && instance.migration_set_sha256 == request.migration_set_sha256
            && instance.protocol_set_sha256 == request.protocol_set_sha256
            && instance.task_queue_sha256 == request.task_queue_sha256
            && instance.failure_converter_sha256 == request.failure_converter_sha256
            && instance.dependency_evidence_sha256 == request.dependency_evidence_sha256,
    )
}

/// Canonical NONSECRET request is the immutable replay body. The transport secret
/// is checked before this function and never stored in an observation or error.
fn nonsecret_heartbeat(request: &wire::HeartbeatRequest) -> ManagedCloudResult<Vec<u8>> {
    let mut value =
        serde_json::to_value(request).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    value
        .as_object_mut()
        .ok_or(ManagedCloudRegistryError::InvalidRequest)?
        .remove("sessionToken");
    managed_cloud_canonical_json(&value)
}

fn stored_heartbeat(encoded: &str, session: &str) -> ManagedCloudResult<wire::HeartbeatRequest> {
    let mut value: serde_json::Value = decode(encoded)?;
    let object = value
        .as_object_mut()
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    ensure(!object.contains_key("sessionToken"))?;
    object.insert(
        "sessionToken".to_owned(),
        serde_json::Value::String(session.to_owned()),
    );
    wire::parse_heartbeat_request(&json(&value)?)
}

fn enum_text<T: serde::Serialize>(value: &T) -> ManagedCloudResult<String> {
    serde_json::to_value(value)
        .map_err(|_| ManagedCloudRegistryError::InvalidRequest)?
        .as_str()
        .map(str::to_owned)
        .ok_or(ManagedCloudRegistryError::InvalidRequest)
}

fn latest_observation(
    tx: &mut ControlTransaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    tables: ManagedCloudRuntimeAuthorityTables,
) -> ManagedCloudResult<Option<Record>> {
    tx.optional(&format!("SELECT o.heartbeat_sequence,o.canonical_heartbeat_base64url,o.heartbeat_at_ms,
        o.observation_sha256,o.serving_latch_sha256 FROM {} h
        JOIN {} o ON o.runtime_instance_id=h.runtime_instance_id AND o.instance_epoch=h.instance_epoch
          AND o.heartbeat_sequence=h.heartbeat_sequence AND o.observation_sha256=h.observation_sha256
        WHERE h.runtime_instance_id=$1 AND h.instance_epoch=$2 AND h.protocol_version=2", tables.heartbeats, observation_table(&instance.role)),
        &args![&instance.runtime_instance_id, instance.instance_epoch])
}

fn current_boot(
    tx: &mut ControlTransaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    boot_id: &str,
    generation: i64,
) -> ManagedCloudResult<()> {
    let row = tx.one("SELECT r.worker_id,r.runtime_identity_sha256 FROM jobs_managed_cloud_source_verifier_boot_registrations r
        JOIN jobs_managed_cloud_source_verifier_boot_generation_heads h ON h.runtime_instance_id=r.runtime_instance_id AND h.instance_epoch=r.instance_epoch
        WHERE r.runtime_instance_id=$1 AND r.instance_epoch=$2 AND r.boot_id=$3 AND r.boot_generation=$4
          AND r.protocol_version=2 AND h.current_boot_generation=r.boot_generation",
        &args![&instance.runtime_instance_id, instance.instance_epoch, boot_id, generation])?;
    ensure(row.text(0)? == instance.worker_id && row.text(1)? == instance.runtime_identity_sha256)
}

fn finite_health(request: &wire::HeartbeatRequest, latch: Option<&str>) -> ManagedCloudResult<()> {
    use wire::{HeartbeatHealthState as H, HeartbeatReasonCode as R, LifecycleState as L};
    ensure(
        matches!(
            (request.health_state, request.reason_code),
            (H::Ready, None) | (H::Draining, Some(R::Draining))
        ) || (request.health_state == H::Degraded
            && request.reason_code.is_some_and(|v| v != R::Draining)),
    )?;
    match request.lifecycle_state {
        L::Parked => {
            ensure(request.acknowledged_latch_sha256.is_none() && request.health_state != H::Ready)
        }
        L::LatchAcknowledged => ensure(
            latch.is_some()
                && request.acknowledged_latch_sha256.as_deref() == latch
                && request.health_state != H::Ready,
        ),
        L::Operational => {
            ensure(latch.is_some() && request.acknowledged_latch_sha256.as_deref() == latch)
        }
    }
}

fn loop_positive(
    state: wire::LoopState,
    progress: wire::ControlProgress,
    phase: wire::LifecycleState,
) -> bool {
    progress == wire::ControlProgress::Fresh
        && match phase {
            wire::LifecycleState::Parked => state == wire::LoopState::Parked,
            wire::LifecycleState::Operational => state == wire::LoopState::Running,
            wire::LifecycleState::LatchAcknowledged => false,
        }
}

fn namespace_fact(namespace: Option<&wire::TemporalNamespace>) -> ManagedCloudResult<bool> {
    if namespace.is_some() {
        // The signed endpoint/namespace-UUID and private-resource proof reader is
        // not implemented yet. A matching name/hash is insufficient to bind ALL
        // endpoint/authority/namespace-ID fields, even in a negative report.
        // Until that reader exists the caller must use the required null for an
        // unavailable dependency, not a partially authenticated reference.
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }
    Ok(false)
}

fn runner_fact(
    tx: &mut ControlTransaction<'_>,
    instance: &ManagedCloudRuntimeInstance,
    value: &wire::VolumeBinding,
    now: i64,
) -> ManagedCloudResult<bool> {
    let row = tx.optional("SELECT 1 FROM jobs_runner_volumes v
        JOIN jobs_runner_volume_keys k ON k.volume_id=v.volume_id AND k.enrollment_epoch=v.current_epoch
        JOIN jobs_runner_process_runtime_bindings b ON b.volume_id=v.volume_id AND b.enrollment_epoch=v.current_epoch AND b.process_instance_id=v.active_instance_id
        JOIN jobs_runner_process_runtime_grants g ON g.grant_id=b.grant_id AND g.runtime_sha256=b.runtime_sha256 AND g.expected_worker_id=b.worker_id
        JOIN jobs_runner_volume_storage_attestations a ON a.volume_id=v.volume_id AND a.enrollment_epoch=v.current_epoch AND a.process_instance_id=v.active_instance_id
        WHERE v.volume_id=$1 AND v.current_epoch=$2 AND v.active_instance_id=$3 AND b.grant_id=$4
          AND b.runtime_sha256=$5 AND a.attestation_generation=$6 AND a.attestation_sha256=$7 AND v.worker_id=$8
          AND b.worker_id=v.worker_id AND v.status='active' AND v.instance_lease_expires_at_ms>$9
          AND k.retired_at_ms IS NULL AND g.expires_at_ms>$9
          AND a.enrollment_generation=v.enrollment_generation AND a.resource_fingerprint=v.resource_fingerprint
          AND a.volume_key_fingerprint=k.key_fingerprint
          AND a.required_tombstone_generation=v.required_tombstone_generation
          AND a.reconciled_tombstone_generation=v.reconciled_tombstone_generation
          AND v.required_tombstone_generation=v.reconciled_tombstone_generation
          AND a.observed_at_ms<=$9 AND a.received_at_ms<=$9
          AND NOT EXISTS(SELECT 1 FROM jobs_runner_process_runtime_grant_revocations r WHERE r.grant_id=g.grant_id)
          AND NOT EXISTS(SELECT 1 FROM jobs_runner_volume_storage_attestations n WHERE n.volume_id=a.volume_id AND n.enrollment_epoch=a.enrollment_epoch AND n.attestation_generation>a.attestation_generation)",
        &args![&value.volume_id, value.enrollment_epoch, &value.process_instance_id, &value.runtime_grant_id,
            &value.runtime_sha256, value.storage_attestation_generation, &value.storage_attestation_sha256, &instance.worker_id, now])?;
    // A supplied reference must join current accepted rows even in a negative report.
    ensure(row.is_some())?;
    Ok(true)
}

fn gateway_fact(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    value: &wire::GatewayControl,
    phase: wire::LifecycleState,
    latch: Option<&str>,
    now: i64,
) -> ManagedCloudResult<bool> {
    referenced_instance(
        tx,
        &auth.authority,
        &value.runtime_instance_id,
        value.instance_epoch,
        "workflow_gateway",
        now,
    )?;
    let row = tx.one("SELECT o.canonical_heartbeat_base64url,o.heartbeat_at_ms,o.serving_latch_sha256
        FROM jobs_managed_cloud_runtime_heartbeats h JOIN jobs_managed_cloud_runtime_observations o
          ON o.runtime_instance_id=h.runtime_instance_id AND o.instance_epoch=h.instance_epoch AND o.observation_sha256=h.observation_sha256
        WHERE h.runtime_instance_id=$1 AND h.instance_epoch=$2 AND o.observation_sha256=$3 AND o.role='workflow_gateway'
          AND o.activation_sha256=$4 AND o.manifest_sha256=$5 AND o.head_revision=$6 AND o.transition_sha256=$7",
        &args![&value.runtime_instance_id, value.instance_epoch, &value.observation_sha256,
            &auth.instance.activation_sha256, &auth.instance.manifest_sha256, auth.instance.head_revision, &auth.instance.transition_sha256])?;
    ensure(
        value.runtime_instance_id != auth.instance.runtime_instance_id
            && value.head_revision == auth.instance.head_revision
            && value.transition_sha256 == auth.instance.transition_sha256,
    )?;
    let request = stored_heartbeat(&row.text(0)?, &"A".repeat(43))?;
    ensure(
        request.authority_binding == auth.authority.binding
            && request.lifecycle_state == value.lifecycle_state,
    )?;
    let fresh = row.integer(1)? <= now && now - row.integer(1)? < auth.authority.heartbeat_ttl_ms;
    if !fresh {
        return Ok(false);
    }
    let appropriate = if phase == wire::LifecycleState::Operational {
        request.lifecycle_state == wire::LifecycleState::Operational
            && request.health_state == wire::HeartbeatHealthState::Ready
            && row.optional_text(2)?.as_deref() == latch
    } else {
        (request.lifecycle_state == wire::LifecycleState::Parked
            && request.health_state == wire::HeartbeatHealthState::Degraded
            && request.reason_code == Some(wire::HeartbeatReasonCode::Startup))
            || (latch.is_some()
                && request.lifecycle_state == wire::LifecycleState::Operational
                && request.health_state == wire::HeartbeatHealthState::Ready
                && row.optional_text(2)?.as_deref() == latch)
    };
    let wire::DependencyProbe::WorkflowGateway(probe) = &request.dependency_probe else {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    };
    let namespace = probe
        .temporal_namespace
        .as_ref()
        .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
    // A supplied G must reference a genuinely validated gateway, not a negative
    // row with no namespace. The missing private-resource reader must bind both
    // this TLS tuple and G's HTTPS endpoint before this reference can qualify.
    let namespace_bound = namespace_fact(Some(namespace))?;
    Ok(appropriate
        && probe.listener_state == wire::ListenerState::Listening
        && namespace_bound
        && api_dependency_current(tx, auth, phase, latch, now)?)
}

fn referenced_instance(
    tx: &mut ControlTransaction<'_>,
    current: &ProvisioningControlAuthority,
    id: &str,
    epoch: i64,
    role: &str,
    now: i64,
) -> ManagedCloudResult<()> {
    let (instance, _, tables) = tx.instance(id)?;
    ensure(
        instance.instance_epoch == epoch
            && instance.role == role
            && instance.scope == current.head.scope
            && instance.activation_sha256 == current.head.activation_sha256
            && instance.manifest_sha256 == current.head.manifest_sha256
            && instance.head_revision == current.head.head_revision
            && instance.transition_sha256 == current.head.transition_sha256,
    )?;
    let (grant, actual_tables) = tx.grant(&instance.grant_id)?;
    ensure(tables == actual_tables)?;
    require_grant_active(tx, current, &grant, tables, now)?;
    if tx.optional("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1 AND subject_kind='runtime_instance'
        AND subject_id=$2 AND subject_sha256=$3 LIMIT 1", &args![now, id, &instance.runtime_identity_sha256])?.is_some() {
        return Err(ManagedCloudRegistryError::Revoked);
    }
    Ok(())
}

fn api_dependency_current(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    phase: wire::LifecycleState,
    latch: Option<&str>,
    now: i64,
) -> ManagedCloudResult<bool> {
    // Dynamic current API join: no downstream rewrite on every API tick. A
    // negative/expired latest API immediately closes the derived gateway proof.
    let rows = tx.rows("SELECT o.canonical_heartbeat_base64url,o.heartbeat_at_ms,o.serving_latch_sha256,o.runtime_instance_id,o.instance_epoch
        FROM jobs_managed_cloud_runtime_heartbeats h JOIN jobs_managed_cloud_runtime_observations o
          ON o.runtime_instance_id=h.runtime_instance_id AND o.instance_epoch=h.instance_epoch AND o.observation_sha256=h.observation_sha256
        JOIN jobs_managed_cloud_runtime_instances i ON i.runtime_instance_id=o.runtime_instance_id AND i.instance_epoch=o.instance_epoch
        JOIN jobs_managed_cloud_runtime_grants g ON g.grant_id=i.grant_id
        WHERE o.activation_sha256=$1 AND o.manifest_sha256=$2 AND o.head_revision=$3 AND o.transition_sha256=$4
          AND o.role='jobs_api' AND i.protocol_version=2 AND g.expires_at_ms>$5
          AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_runtime_grant_revocations r WHERE r.grant_id=g.grant_id)",
        &args![&auth.instance.activation_sha256, &auth.instance.manifest_sha256, auth.instance.head_revision, &auth.instance.transition_sha256, now])?;
    for row in rows {
        let at = row.integer(1)?;
        if at > now || now - at >= auth.authority.heartbeat_ttl_ms {
            continue;
        }
        // A revoked/expired dependency never contributes. It is not an invalid
        // caller reference here: the gateway supplied no API identity at all.
        match referenced_instance(
            tx,
            &auth.authority,
            &row.text(3)?,
            row.integer(4)?,
            "jobs_api",
            now,
        ) {
            Ok(()) => {}
            Err(ManagedCloudRegistryError::Revoked | ManagedCloudRegistryError::GrantExpired) => {
                continue
            }
            Err(error) => return Err(error),
        }
        let request = stored_heartbeat(&row.text(0)?, &"A".repeat(43))?;
        if request.authority_binding != auth.authority.binding {
            continue;
        }
        let wire::DependencyProbe::JobsApi(probe) = request.dependency_probe else {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        };
        let operational = request.lifecycle_state == wire::LifecycleState::Operational
            && request.health_state == wire::HeartbeatHealthState::Ready
            && latch.is_some()
            && request.acknowledged_latch_sha256.as_deref() == latch
            && row.optional_text(2)?.as_deref() == latch;
        let parked = request.lifecycle_state == wire::LifecycleState::Parked
            && request.health_state == wire::HeartbeatHealthState::Degraded
            && request.reason_code == Some(wire::HeartbeatReasonCode::Startup);
        if probe.listener_state == wire::ListenerState::Listening
            && probe.server_state == wire::ServerState::Running
            && (operational || (phase == wire::LifecycleState::Parked && parked))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn positive_probe(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    request: &wire::HeartbeatRequest,
    latch: Option<&str>,
    now: i64,
    startup: Option<&wire::BootStartupRequest>,
) -> ManagedCloudResult<bool> {
    use wire::DependencyProbe as P;
    let phase = request.lifecycle_state;
    match &request.dependency_probe {
        P::JobsApi(v) => Ok(v.listener_state == wire::ListenerState::Listening
            && v.server_state == wire::ServerState::Running),
        P::DiscoveryWorker(v) | P::GlobalDiscoveryWorker(v) => {
            Ok(loop_positive(v.loop_state, v.control_progress, phase))
        }
        P::WorkflowGateway(v) => {
            let namespace = namespace_fact(v.temporal_namespace.as_ref())?;
            let api = api_dependency_current(tx, auth, phase, latch, now)?;
            Ok(v.listener_state == wire::ListenerState::Listening && namespace && api)
        }
        P::WorkflowWorker(v) => {
            namespace_fact(v.temporal_namespace.as_ref())?;
            // Physical queue allocation/principals/isolation + real poller/backlog
            // producer/reader is still missing. Self-reported zero is not proof.
            Ok(false)
        }
        P::WorkflowCommandDispatcher(v) | P::WorkflowCleanupDispatcher(v) => {
            let gateway = v
                .gateway_control
                .as_ref()
                .map(|g| gateway_fact(tx, auth, g, phase, latch, now))
                .transpose()?
                .unwrap_or(false);
            Ok(loop_positive(v.loop_state, v.control_progress, phase) && gateway)
        }
        P::ManagedRunner(v) => {
            let current = v
                .volume_binding
                .as_ref()
                .map(|b| runner_fact(tx, &auth.instance, b, now))
                .transpose()?
                .unwrap_or(false);
            Ok(v.listener_state == wire::ListenerState::Listening
                && v.storage_recovery_complete
                && current)
        }
        P::OriginalSourceVerifier(v) => {
            let Some(binding) = &v.boot_startup else {
                return Ok(false);
            };
            current_boot(
                tx,
                &auth.instance,
                &binding.boot_id,
                binding.boot_generation,
            )?;
            ensure(
                request.boot_id.as_deref() == Some(&binding.boot_id)
                    && request.boot_generation == Some(binding.boot_generation),
            )?;
            let startup_ready = if let Some(startup) = startup {
                ensure(
                    startup.startup_request_id == binding.startup_request_id
                        && startup.boot_id == binding.boot_id
                        && startup.boot_generation == binding.boot_generation
                        && startup.prior_heartbeat_sequence + 1
                            == binding.parked_heartbeat_sequence
                        && request.heartbeat_sequence == binding.parked_heartbeat_sequence,
                )?;
                true
            } else {
                tx.optional("SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities
                    WHERE runtime_instance_id=$1 AND instance_epoch=$2 AND boot_id=$3 AND boot_generation=$4
                      AND startup_request_id=$5 AND parked_heartbeat_sequence=$6 AND protocol_version=2 AND dependency_result='dependency_ready'",
                    &args![&auth.instance.runtime_instance_id, auth.instance.instance_epoch, &binding.boot_id,
                        binding.boot_generation, &binding.startup_request_id, binding.parked_heartbeat_sequence])?.is_some()
            };
            Ok(startup_ready && loop_positive(v.loop_state, v.control_progress, phase))
        }
    }
}

fn parked_predecessor(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    request: &wire::HeartbeatRequest,
    latch: Option<&str>,
    now: i64,
) -> ManagedCloudResult<Option<(i64, String)>> {
    let row = tx.optional(
        &format!(
            "SELECT o.heartbeat_sequence,o.observation_sha256,o.canonical_heartbeat_base64url,
             o.serving_latch_sha256,o.parked_predecessor_sequence,o.parked_predecessor_sha256
        FROM {} h JOIN {} o ON o.runtime_instance_id=h.runtime_instance_id AND o.instance_epoch=h.instance_epoch
          AND o.heartbeat_sequence=h.heartbeat_sequence AND o.observation_sha256=h.observation_sha256
        WHERE h.runtime_instance_id=$1 AND h.instance_epoch=$2 AND h.heartbeat_sequence=$3 AND h.protocol_version=2",
            auth.tables.heartbeats, observation_table(&request.role)
        ),
        &args![
            &request.runtime_instance_id,
            auth.instance.instance_epoch,
            request.heartbeat_sequence - 1
        ],
    )?;
    let Some(row) = row else {
        return Ok(None);
    };
    let prior = stored_heartbeat(&row.text(2)?, &request.session_token)?;
    require_heartbeat_identity(&auth.instance, &prior)?;
    ensure(prior.authority_binding == request.authority_binding)?;
    if prior.boot_id != request.boot_id || prior.boot_generation != request.boot_generation {
        return Ok(None);
    }
    // ONLY latest may establish or carry the chain. A negative, optional ACK,
    // or nonqualifying parked row breaks it; never search past that boundary.
    let positive_parked = prior.lifecycle_state == wire::LifecycleState::Parked
        && prior.health_state == wire::HeartbeatHealthState::Degraded
        && prior.reason_code == Some(wire::HeartbeatReasonCode::Startup)
        && prior.acknowledged_latch_sha256.is_none();
    if positive_parked {
        return if positive_probe(tx, auth, &prior, latch, now, None)? {
            Ok(Some((row.integer(0)?, row.text(1)?)))
        } else {
            Ok(None)
        };
    }
    let positive_operational = prior.lifecycle_state == wire::LifecycleState::Operational
        && prior.health_state == wire::HeartbeatHealthState::Ready
        && prior.reason_code.is_none()
        && latch.is_some()
        && prior.acknowledged_latch_sha256.as_deref() == latch
        && row.optional_text(3)?.as_deref() == latch;
    if !positive_operational || !positive_probe(tx, auth, &prior, latch, now, None)? {
        return Ok(None);
    }
    let (Some(sequence), Some(digest)) = (row.optional_integer(4)?, row.optional_text(5)?) else {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    };
    ensure(sequence < prior.heartbeat_sequence)?;
    let parked = tx.one(
        &format!(
            "SELECT canonical_heartbeat_base64url FROM {} WHERE runtime_instance_id=$1
        AND instance_epoch=$2 AND heartbeat_sequence=$3 AND observation_sha256=$4
        AND lifecycle_state='parked' AND health_state='degraded' AND reason_code='startup'",
            observation_table(&request.role)
        ),
        &args![
            &request.runtime_instance_id,
            auth.instance.instance_epoch,
            sequence,
            &digest
        ],
    )?;
    let parked = stored_heartbeat(&parked.text(0)?, &request.session_token)?;
    require_heartbeat_identity(&auth.instance, &parked)?;
    ensure(
        parked.authority_binding == request.authority_binding
            && parked.boot_id == request.boot_id
            && parked.boot_generation == request.boot_generation
            && parked.acknowledged_latch_sha256.is_none()
            && parked.heartbeat_sequence == sequence
            && parked.lifecycle_state == wire::LifecycleState::Parked
            && parked.health_state == wire::HeartbeatHealthState::Degraded
            && parked.reason_code == Some(wire::HeartbeatReasonCode::Startup),
    )?;
    // Inherit the SAME immutable tuple across ordinary Ready ticks. The latest
    // Ready and the new report are revalidated; no parked-before-every-tick loop.
    Ok(Some((sequence, digest)))
}

fn contaminate(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    request: &wire::HeartbeatRequest,
    digest: &str,
    now: i64,
) -> ManagedCloudResult<()> {
    let Some(queue) = &request.queue_observation else {
        return Ok(());
    };
    if request.role != "workflow_worker" || queue.delivered_task_count == 0 {
        return Ok(());
    }
    let h = &auth.authority.head;
    // The identity binds this immutable authenticated observation, never task data.
    // No payload/PII is copied into the contamination table.
    tx.execute("INSERT INTO jobs_managed_cloud_queue_contaminations(activation_sha256,manifest_sha256,
        environment,region,channel,authority_kind,purpose,context_sha256,queue_generation_id,task_queue_sha256,
        head_revision,transition_sha256,runtime_instance_id,instance_epoch,worker_id,role,delivery_identity_sha256,reason_code,observed_at_ms)
        VALUES($1,$2,$3,$4,$5,'provisioning_v4','production',NULL,$6,$7,$8,$9,$10,$11,$12,'workflow_worker',$13,'unexpected_pre_serving_delivery',$14)
        ON CONFLICT(queue_generation_id,runtime_instance_id,instance_epoch,delivery_identity_sha256) DO NOTHING",
        &args![&h.activation_sha256, &h.manifest_sha256, &h.scope.environment, &h.scope.region, &h.scope.channel,
            &auth.authority.binding.queue_generation_id, &h.task_queue_sha256, h.head_revision, &h.transition_sha256,
            &request.runtime_instance_id, auth.instance.instance_epoch, &request.worker_id, digest, now])?;
    Ok(())
}

const OBSERVATION_COLUMNS: &str = "runtime_instance_id,instance_epoch,heartbeat_sequence,observation_sha256,grant_id,runtime_identity_sha256,
    protocol_version,activation_sha256,manifest_sha256,environment,region,channel,authority_kind,purpose,context_sha256,
    queue_generation_id,task_queue_sha256,head_revision,transition_sha256,component_id,role,worker_id,artifact_sha256,
    config_schema_sha256,migration_set_sha256,protocol_set_sha256,failure_converter_sha256,dependency_evidence_sha256,
    lifecycle_state,health_state,reason_code,acknowledged_latch_sha256,serving_latch_sha256,canonical_heartbeat_base64url,
    dependency_probe_base64url,poller_identity_sha256,backlog_count,delivered_task_count,parked_predecessor_sequence,parked_predecessor_sha256,heartbeat_at_ms";

fn write_observation(
    tx: &mut ControlTransaction<'_>,
    auth: &AuthenticatedInstance,
    request: &wire::HeartbeatRequest,
    now: i64,
    startup: Option<&wire::BootStartupRequest>,
) -> ManagedCloudResult<wire::HeartbeatResponse> {
    require_heartbeat_identity(&auth.instance, request)?;
    ensure(request.authority_binding == auth.authority.binding)?;
    if request.role == "original_source_verifier" {
        let (Some(boot), Some(generation)) = (&request.boot_id, request.boot_generation) else {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        };
        current_boot(tx, &auth.instance, boot, generation)?;
    }
    let previous = latest_observation(tx, &auth.instance, auth.tables)?;
    let previous_sequence = previous
        .as_ref()
        .map(|r| r.integer(0))
        .transpose()?
        .unwrap_or(0);
    let canonical = nonsecret_heartbeat(request)?;
    if request.heartbeat_sequence == previous_sequence {
        let row = previous.ok_or(ManagedCloudRegistryError::HeartbeatSequenceConflict)?;
        let stored_bytes = managed_cloud_decode_base64url(&row.text(1)?)?;
        if stored_bytes != canonical {
            return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
        }
        let digest = wire::observation_sha256_v2(
            &auth.instance.grant_id,
            request,
            row.integer(2)?,
            auth.instance.instance_epoch,
            row.optional_text(4)?.as_deref(),
        )?;
        ensure(digest == row.text(3)?)?;
        return Ok(wire::HeartbeatResponse::from_request(
            request,
            auth.instance.instance_epoch,
            row.integer(2)?,
            row.optional_text(4)?,
            digest,
            true,
        ));
    }
    if previous_sequence.checked_add(1) != Some(request.heartbeat_sequence) {
        return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
    }
    let latch = current_latch(tx, &auth.authority)?;
    finite_health(request, latch.as_deref())?;
    let probe_positive = positive_probe(tx, auth, request, latch.as_deref(), now, startup)?;
    let is_ready = request.health_state == wire::HeartbeatHealthState::Ready;
    // A contradictory Ready report is rejected, never silently downgraded.
    if is_ready {
        ensure(probe_positive)?;
    }
    let predecessor = if is_ready {
        let prior = parked_predecessor(tx, auth, request, latch.as_deref(), now)?;
        ensure(prior.is_some())?;
        prior
    } else {
        None
    };
    if let Some(queue) = &request.queue_observation {
        if is_ready {
            ensure(queue.backlog_count == 0 && queue.delivered_task_count == 0)?;
        }
    }
    let digest = wire::observation_sha256_v2(
        &auth.instance.grant_id,
        request,
        now,
        auth.instance.instance_epoch,
        latch.as_deref(),
    )?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical);
    let phase = enum_text(&request.lifecycle_state)?;
    let health = enum_text(&request.health_state)?;
    let reason = request
        .reason_code
        .map(|value| enum_text(&value))
        .transpose()?;
    let queue = request.queue_observation.as_ref();
    let columns = OBSERVATION_COLUMNS;
    let mut values = Vec::from(args![
        &request.runtime_instance_id,
        auth.instance.instance_epoch,
        request.heartbeat_sequence,
        &digest,
        &auth.instance.grant_id,
        &auth.instance.runtime_identity_sha256,
        2_i64,
        &request.activation_sha256,
        &request.manifest_sha256,
        &auth.instance.scope.environment,
        &auth.instance.scope.region,
        &auth.instance.scope.channel,
        "provisioning_v4",
        "production",
        auth.authority.binding.context_sha256.as_deref(),
        &auth.authority.binding.queue_generation_id,
        &request.task_queue_sha256,
        request.observed_head_revision,
        &request.observed_transition_sha256,
        &request.component_id,
        &request.role,
        &request.worker_id,
        &request.artifact_sha256,
        &request.config_schema_sha256,
        &request.migration_set_sha256,
        &request.protocol_set_sha256,
        &request.failure_converter_sha256,
        &request.dependency_evidence_sha256,
        &phase,
        &health,
        reason.as_deref(),
        request.acknowledged_latch_sha256.as_deref(),
        latch.as_deref(),
        &encoded,
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(wire::canonical_probe_bytes(&request.dependency_probe)?),
        queue.map(|v| v.poller_identity_sha256.as_str()),
        queue.map(|v| v.backlog_count),
        queue.map(|v| v.delivered_task_count),
        predecessor.as_ref().map(|v| v.0),
        predecessor.as_ref().map(|v| v.1.as_str()),
        now
    ]);
    let (all_columns, total) = if request.role == "original_source_verifier" {
        values.push(Argument::from(request.boot_id.as_deref()));
        values.push(Argument::from(request.boot_generation));
        (format!("{columns},boot_id,boot_generation"), 43)
    } else {
        (columns.to_owned(), 41)
    };
    let placeholders = (1..=total)
        .map(|v| format!("${v}"))
        .collect::<Vec<_>>()
        .join(",");
    tx.execute(
        &format!(
            "INSERT INTO {}({all_columns}) VALUES({placeholders})",
            observation_table(&request.role)
        ),
        &values,
    )?;
    contaminate(tx, auth, request, &digest, now)?;
    let latest_values = args![
        &request.runtime_instance_id,
        auth.instance.instance_epoch,
        request.heartbeat_sequence,
        &request.activation_sha256,
        &request.manifest_sha256,
        &request.component_id,
        &request.role,
        &request.worker_id,
        &request.artifact_sha256,
        request.observed_head_revision,
        &request.observed_transition_sha256,
        &request.migration_set_sha256,
        &request.config_schema_sha256,
        &request.protocol_set_sha256,
        &request.task_queue_sha256,
        &request.failure_converter_sha256,
        &request.dependency_evidence_sha256,
        &health,
        reason.as_deref(),
        now,
        2_i64,
        &digest,
        previous_sequence
    ];
    let changed = if previous_sequence == 0 {
        tx.execute(&format!("INSERT INTO {}(runtime_instance_id,instance_epoch,heartbeat_sequence,activation_sha256,
            manifest_sha256,component_id,role,worker_id,artifact_sha256,observed_head_revision,observed_transition_sha256,
            migration_set_sha256,config_schema_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,
            dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms,protocol_version,observation_sha256)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)", auth.tables.heartbeats), &latest_values[..22])?
    } else {
        // Identity columns are immutable instance bindings; bind every value to
        // satisfy the exact latest->permanent observation trigger on both DBs.
        tx.execute(&format!("UPDATE {} SET heartbeat_sequence=$3,activation_sha256=$4,manifest_sha256=$5,component_id=$6,
            role=$7,worker_id=$8,artifact_sha256=$9,observed_head_revision=$10,observed_transition_sha256=$11,
            migration_set_sha256=$12,config_schema_sha256=$13,protocol_set_sha256=$14,task_queue_sha256=$15,
            failure_converter_sha256=$16,dependency_evidence_sha256=$17,health_state=$18,reason_code=$19,
            heartbeat_at_ms=$20,protocol_version=$21,observation_sha256=$22
            WHERE runtime_instance_id=$1 AND instance_epoch=$2 AND heartbeat_sequence=$23", auth.tables.heartbeats), &latest_values)?
    };
    if changed != 1 {
        return Err(ManagedCloudRegistryError::HeartbeatSequenceConflict);
    }
    tx.execute(&format!("INSERT INTO {}(runtime_instance_id,instance_epoch,heartbeat_sequence,activation_sha256,manifest_sha256,
        component_id,role,worker_id,artifact_sha256,observed_head_revision,observed_transition_sha256,migration_set_sha256,
        config_schema_sha256,protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,
        health_state,reason_code,heartbeat_at_ms)
        SELECT runtime_instance_id,instance_epoch,heartbeat_sequence,activation_sha256,manifest_sha256,component_id,role,
        worker_id,artifact_sha256,observed_head_revision,observed_transition_sha256,migration_set_sha256,config_schema_sha256,
        protocol_set_sha256,task_queue_sha256,failure_converter_sha256,dependency_evidence_sha256,health_state,reason_code,heartbeat_at_ms
        FROM {} WHERE runtime_instance_id=$1 AND instance_epoch=$2", auth.tables.heartbeat_audit, auth.tables.heartbeats),
        &args![&request.runtime_instance_id, auth.instance.instance_epoch])?;
    tx.execute(
        &format!(
            "DELETE FROM {} WHERE runtime_instance_id=$1 AND instance_epoch=$2
        AND heartbeat_sequence<=CAST($3 AS BIGINT)-CAST($4 AS BIGINT)",
            auth.tables.heartbeat_audit
        ),
        &args![
            &request.runtime_instance_id,
            auth.instance.instance_epoch,
            request.heartbeat_sequence,
            MANAGED_CLOUD_HEARTBEAT_AUDIT_LIMIT
        ],
    )?;
    if is_ready && request.role == "original_source_verifier" {
        let generation = request
            .boot_generation
            .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
        let changed = tx.execute("UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads
            SET ready_boot_generation=$3,updated_at_ms=$4 WHERE runtime_instance_id=$1 AND instance_epoch=$2
              AND current_boot_generation=$3 AND (ready_boot_generation IS NULL OR ready_boot_generation<=$3)",
            &args![&request.runtime_instance_id, auth.instance.instance_epoch, generation, now])?;
        ensure(changed == 1)?;
    }
    Ok(wire::HeartbeatResponse::from_request(
        request,
        auth.instance.instance_epoch,
        now,
        latch,
        digest,
        false,
    ))
}

pub(crate) fn record_provisioning_runtime_heartbeat(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    input: &wire::HeartbeatRequest,
) -> ManagedCloudResult<wire::HeartbeatResponse> {
    let input = wire::parse_heartbeat_request(&json(input)?)?;
    process.permits(&input.authority_binding)?;
    transaction(pool, |tx| {
        let now = tx.now()?;
        let auth = authenticated_instance(
            tx,
            process,
            &input.runtime_instance_id,
            &input.worker_id,
            &input.session_token,
            &input.authority_binding,
            now,
        )?;
        write_observation(tx, &auth, &input, now, None)
    })
}

const BOOT_REGISTRATION_REQUEST_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-source-verifier-boot-registration-request-v2\0";
const BOOT_REGISTRATION_RESPONSE_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-source-verifier-boot-registration-response-v2\0";
const BOOT_STARTUP_REQUEST_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-source-verifier-boot-startup-request-v2\0";
const BOOT_STARTUP_RESPONSE_DOMAIN: &[u8] =
    b"bluey-jobs-managed-cloud-source-verifier-boot-startup-response-v2\0";

fn boot_digest<T: serde::Serialize>(
    domain: &[u8],
    value: &T,
    omit_session: bool,
) -> ManagedCloudResult<String> {
    let mut value =
        serde_json::to_value(value).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    if omit_session {
        ensure(
            value
                .as_object_mut()
                .ok_or(ManagedCloudRegistryError::InvalidRequest)?
                .remove("sessionToken")
                .is_some(),
        )?;
    }
    let mut bytes = domain.to_vec();
    bytes.extend(managed_cloud_canonical_json(&value)?);
    Ok(managed_cloud_sha256(&bytes))
}

fn startup_replay_row(
    tx: &mut ControlTransaction<'_>,
    id: &str,
) -> ManagedCloudResult<Option<Record>> {
    // LEFT JOIN keeps a conflicting existing request visible even if its
    // referenced immutable observation is missing/corrupt. Never treat that as
    // permission to execute startup again.
    tx.optional("SELECT s.request_sha256,s.canonical_response_base64url,s.response_sha256,s.protocol_version,
        o.canonical_heartbeat_base64url,o.instance_epoch,o.heartbeat_at_ms,o.serving_latch_sha256,o.observation_sha256,
        s.startup_at_ms,s.observation_sha256,s.parked_heartbeat_sequence,s.prior_heartbeat_sequence,
        s.boot_id,s.boot_generation,o.grant_id,o.runtime_identity_sha256,o.dependency_probe_base64url,
        o.boot_id,o.boot_generation,s.runtime_instance_id,s.instance_epoch,s.worker_id,o.runtime_instance_id
        FROM jobs_managed_cloud_source_verifier_boot_startup_authorities s
        LEFT JOIN jobs_managed_cloud_osv_runtime_observations o ON o.runtime_instance_id=s.runtime_instance_id
          AND o.instance_epoch=s.instance_epoch AND o.heartbeat_sequence=s.parked_heartbeat_sequence
          AND o.observation_sha256=s.observation_sha256 AND o.boot_id=s.boot_id AND o.boot_generation=s.boot_generation
        WHERE s.startup_request_id=$1", &args![id])
}

fn startup_replay_response(
    row: &Record,
    auth: &AuthenticatedInstance,
    input: &wire::BootStartupRequest,
) -> ManagedCloudResult<wire::HeartbeatResponse> {
    if row.integer(3)? != 2
        || row.text(0)? != boot_digest(BOOT_STARTUP_REQUEST_DOMAIN, input, true)?
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let heartbeat = stored_heartbeat(&row.text(4)?, &input.session_token)?;
    require_heartbeat_identity(&auth.instance, &heartbeat)?;
    let stored_probe_bytes = managed_cloud_decode_base64url(&row.text(17)?)?;
    let stored_probe = wire::parse_dependency_probe(&stored_probe_bytes)?;
    ensure(
        heartbeat.authority_binding == input.authority_binding
            && heartbeat.authority_binding == auth.authority.binding
            && row.integer(5)? == auth.instance.instance_epoch
            && row.integer(6)? == row.integer(9)?
            && row.text(8)? == row.text(10)?
            && heartbeat.heartbeat_sequence == row.integer(11)?
            && row.integer(12)? == input.prior_heartbeat_sequence
            && row.integer(11)? == input.prior_heartbeat_sequence + 1
            && row.text(13)? == input.boot_id
            && row.integer(14)? == input.boot_generation
            && row.text(15)? == auth.instance.grant_id
            && row.text(16)? == auth.instance.runtime_identity_sha256
            && stored_probe == heartbeat.dependency_probe
            && row.text(18)? == input.boot_id
            && row.integer(19)? == input.boot_generation
            && row.text(20)? == input.runtime_instance_id
            && row.integer(21)? == input.instance_epoch
            && row.text(22)? == input.worker_id
            && row.text(23)? == input.runtime_instance_id
            && heartbeat.boot_id.as_deref() == Some(input.boot_id.as_str())
            && heartbeat.boot_generation == Some(input.boot_generation)
            && heartbeat.dependency_probe == input.dependency_probe
            && heartbeat.lifecycle_state == wire::LifecycleState::Parked
            && heartbeat.health_state == wire::HeartbeatHealthState::Degraded
            && heartbeat.reason_code == Some(wire::HeartbeatReasonCode::Startup)
            && heartbeat.acknowledged_latch_sha256.is_none()
            && heartbeat.queue_observation.is_none(),
    )?;
    let digest = wire::observation_sha256_v2(
        &auth.instance.grant_id,
        &heartbeat,
        row.integer(6)?,
        row.integer(5)?,
        row.optional_text(7)?.as_deref(),
    )?;
    ensure(digest == row.text(8)?)?;
    let expected = wire::HeartbeatResponse::from_request(
        &heartbeat,
        row.integer(5)?,
        row.integer(6)?,
        row.optional_text(7)?,
        digest,
        false,
    );
    // A self-consistent response/hash is insufficient. Bind ALL 29 fields to the
    // actual FK-bound permanent observation and original startup DB time.
    ensure(
        row.text(1)? == encode(&expected)?
            && row.text(2)? == boot_digest(BOOT_STARTUP_RESPONSE_DOMAIN, &expected, false)?,
    )?;
    Ok(expected)
}

pub(crate) fn register_provisioning_source_verifier_boot(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    input: &wire::BootRegistrationRequest,
) -> ManagedCloudResult<wire::BootRegistrationResponse> {
    let input = wire::parse_boot_registration_request(&json(input)?)?;
    process.permits(&input.authority_binding)?;
    transaction(pool, |tx| {
        let now = tx.now()?;
        let auth = authenticated_instance(
            tx,
            process,
            &input.runtime_instance_id,
            &input.worker_id,
            &input.session_token,
            &input.authority_binding,
            now,
        )?;
        let instance = &auth.instance;
        ensure(
            instance.instance_epoch == input.instance_epoch
                && instance.role == input.role
                && instance.component_id == input.component_id
                && instance.runtime_identity_sha256 == input.runtime_identity_sha256
                && instance.activation_sha256 == input.activation_sha256
                && instance.manifest_sha256 == input.manifest_sha256,
        )?;
        // Session was authenticated above. The stored hash contains no token or
        // token hash, and a changed nonsecret request conflicts under the same ID.
        let request_hash = boot_digest(BOOT_REGISTRATION_REQUEST_DOMAIN, &input, true)?;
        if let Some(row) = tx.optional("SELECT registration_request_sha256,canonical_response_base64url,response_sha256,protocol_version,boot_generation,registered_at_ms
            FROM jobs_managed_cloud_source_verifier_boot_registrations WHERE registration_request_id=$1", &args![&input.registration_request_id])? {
            if row.integer(3)? != 2 || row.text(0)? != request_hash { return Err(ManagedCloudRegistryError::IdentityConflict); }
            let response: wire::BootRegistrationResponse = decode(&row.text(1)?)?;
            ensure(boot_digest(BOOT_REGISTRATION_RESPONSE_DOMAIN, &response, false)? == row.text(2)?)?;
            ensure(response.version == 2 && response.authority_binding == input.authority_binding
                && response.registration_request_id == input.registration_request_id && response.boot_id == input.boot_id
                && response.runtime_instance_id == input.runtime_instance_id && response.instance_epoch == input.instance_epoch
                && response.worker_id == input.worker_id && response.runtime_identity_sha256 == input.runtime_identity_sha256
                && response.activation_sha256 == input.activation_sha256 && response.manifest_sha256 == input.manifest_sha256
                && response.component_id == input.component_id && response.role == input.role
                && response.boot_generation == row.integer(4)? && response.registered_at_ms == row.integer(5)?)?;
            return Ok(response);
        }
        if tx.optional("SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_registrations WHERE boot_id=$1", &args![&input.boot_id])?.is_some() {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_boot_generation_heads(runtime_instance_id,instance_epoch,
            current_boot_generation,ready_boot_generation,updated_at_ms) VALUES($1,$2,1,NULL,$3)
            ON CONFLICT(runtime_instance_id,instance_epoch) DO UPDATE SET
              current_boot_generation=jobs_managed_cloud_source_verifier_boot_generation_heads.current_boot_generation+1,
              updated_at_ms=$3", &args![&input.runtime_instance_id, input.instance_epoch, now])?;
        let generation = tx.one("SELECT current_boot_generation FROM jobs_managed_cloud_source_verifier_boot_generation_heads
            WHERE runtime_instance_id=$1 AND instance_epoch=$2", &args![&input.runtime_instance_id, input.instance_epoch])?.integer(0)?;
        ensure(managed_cloud_safe_integer(generation, true))?;
        let response = wire::BootRegistrationResponse {
            version: 2,
            authority_binding: input.authority_binding.clone(),
            registration_request_id: input.registration_request_id.clone(),
            boot_id: input.boot_id.clone(),
            runtime_instance_id: input.runtime_instance_id.clone(),
            instance_epoch: input.instance_epoch,
            worker_id: input.worker_id.clone(),
            runtime_identity_sha256: input.runtime_identity_sha256.clone(),
            activation_sha256: input.activation_sha256.clone(),
            manifest_sha256: input.manifest_sha256.clone(),
            component_id: input.component_id.clone(),
            role: input.role.clone(),
            boot_generation: generation,
            registered_at_ms: now,
        };
        let response_hash = boot_digest(BOOT_REGISTRATION_RESPONSE_DOMAIN, &response, false)?;
        tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_boot_registrations(boot_id,registration_request_id,
            runtime_instance_id,instance_epoch,boot_generation,worker_id,runtime_identity_sha256,activation_sha256,manifest_sha256,
            component_id,role,registration_request_sha256,registration_authority_sha256,canonical_response_base64url,response_sha256,
            registered_at_ms,protocol_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,2)",
            &args![&input.boot_id, &input.registration_request_id, &input.runtime_instance_id, input.instance_epoch, generation,
                &input.worker_id, &input.runtime_identity_sha256, &input.activation_sha256, &input.manifest_sha256,
                &input.component_id, &input.role, request_hash, &response_hash, encode(&response)?, &response_hash, now])?;
        // registration_authority_sha256 is the same domain-separated response
        // identity in v2, NOT a fifth domain, signature or legacy READY authority.
        Ok(response)
    })
}

pub(crate) fn record_provisioning_source_verifier_startup(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    input: &wire::BootStartupRequest,
) -> ManagedCloudResult<wire::HeartbeatResponse> {
    let input = wire::parse_boot_startup_request(&json(input)?)?;
    process.permits(&input.authority_binding)?;
    transaction(pool, |tx| {
        let now = tx.now()?;
        let auth = authenticated_instance(
            tx,
            process,
            &input.runtime_instance_id,
            &input.worker_id,
            &input.session_token,
            &input.authority_binding,
            now,
        )?;
        ensure(
            auth.instance.role == "original_source_verifier"
                && auth.instance.instance_epoch == input.instance_epoch,
        )?;
        let request_hash = boot_digest(BOOT_STARTUP_REQUEST_DOMAIN, &input, true)?;
        if let Some(row) = startup_replay_row(tx, &input.startup_request_id)? {
            // Immutable original response, including replayed:false. No fresh
            // probe, clock, sequence, latch or TTL is substituted on exact retry.
            return startup_replay_response(&row, &auth, &input);
        }
        current_boot(tx, &auth.instance, &input.boot_id, input.boot_generation)?;
        if tx.optional("SELECT 1 FROM jobs_managed_cloud_source_verifier_boot_startup_authorities WHERE boot_id=$1",
            &args![&input.boot_id])?.is_some() { return Err(ManagedCloudRegistryError::IdentityConflict); }
        let row = latest_observation(tx, &auth.instance, auth.tables)?
            .ok_or(ManagedCloudRegistryError::HeartbeatSequenceConflict)?;
        let mut prior = stored_heartbeat(&row.text(1)?, &input.session_token)?;
        ensure(
            row.integer(0)? == input.prior_heartbeat_sequence
                && prior.lifecycle_state == wire::LifecycleState::Parked
                && prior.health_state == wire::HeartbeatHealthState::Degraded
                && prior.boot_id.as_deref() == Some(input.boot_id.as_str())
                && prior.boot_generation == Some(input.boot_generation),
        )?;
        let wire::DependencyProbe::OriginalSourceVerifier(probe) = &input.dependency_probe else {
            return Err(ManagedCloudRegistryError::InvalidAuthority);
        };
        let Some(boot) = &probe.boot_startup else {
            return Err(ManagedCloudRegistryError::Unavailable);
        };
        ensure(
            boot.boot_id == input.boot_id
                && boot.boot_generation == input.boot_generation
                && boot.startup_request_id == input.startup_request_id
                && boot.parked_heartbeat_sequence == input.prior_heartbeat_sequence + 1,
        )?;
        // This is a new work-free control dependency path: usable authenticated
        // DB/head/grant/session/current boot + measured process's fresh PARKED
        // control-tick report. It does not probe assignment or leaseability.
        // The real Node tick producer remains required runtime qualification.
        if !loop_positive(
            probe.loop_state,
            probe.control_progress,
            wire::LifecycleState::Parked,
        ) {
            return Err(ManagedCloudRegistryError::Unavailable);
        }
        prior.heartbeat_sequence = input.prior_heartbeat_sequence + 1;
        prior.dependency_probe = input.dependency_probe.clone();
        prior.health_state = wire::HeartbeatHealthState::Degraded;
        prior.reason_code = Some(wire::HeartbeatReasonCode::Startup);
        prior.lifecycle_state = wire::LifecycleState::Parked;
        prior.acknowledged_latch_sha256 = None;
        ensure(positive_probe(tx, &auth, &prior, None, now, Some(&input))?)?;
        let response = write_observation(tx, &auth, &prior, now, Some(&input))?;
        ensure(!response.replayed)?;
        let response_hash = boot_digest(BOOT_STARTUP_RESPONSE_DOMAIN, &response, false)?;
        tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities(startup_request_id,boot_id,
            runtime_instance_id,instance_epoch,boot_generation,worker_id,request_sha256,degraded_heartbeat_sequence,
            ready_heartbeat_sequence,probe_result,startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms,
            protocol_version,prior_heartbeat_sequence,parked_heartbeat_sequence,dependency_result,observation_sha256)
            VALUES($1,$2,$3,$4,$5,$6,$7,NULL,NULL,NULL,NULL,$8,$9,$10,2,$11,$12,'dependency_ready',$13)",
            &args![&input.startup_request_id, &input.boot_id, &input.runtime_instance_id, input.instance_epoch,
                input.boot_generation, &input.worker_id, request_hash, encode(&response)?, response_hash, now,
                input.prior_heartbeat_sequence, prior.heartbeat_sequence, &response.observation_sha256])?;
        // ready_boot_generation is deliberately NOT advanced by startup. Its
        // later operational path remains fenced by the reviewed signed latch.
        Ok(response)
    })
}

pub(crate) fn issue_provisioning_runtime_grant(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    input: &NewManagedCloudRuntimeGrant,
) -> ManagedCloudResult<ManagedCloudRuntimeGrant> {
    validate_new_managed_cloud_runtime_grant(input)?;
    transaction(pool, |tx| {
        let now = tx.now()?;
        let current = authority(tx, process, &input.scope, now)?;
        let tables = managed_cloud_runtime_tables_for_role(&input.role);
        if let Some((existing, stored_tables)) = tx.grant_by_reference(&input.issuance_ref)? {
            require_exact_managed_cloud_runtime_grant_replay(&existing, input)?;
            require_grant_active(tx, &current, &existing, stored_tables, now)?;
            let token = match tx {
                ControlTransaction::Sqlite(tx) => {
                    sqlite_managed_cloud_replay_grant_token(tx, &existing, now, stored_tables)
                }
                ControlTransaction::Postgres(tx) => {
                    postgres_managed_cloud_replay_grant_token(tx, &existing, now, stored_tables)
                }
            }?;
            return Ok(managed_cloud_runtime_grant_result(
                existing,
                token.as_deref(),
                true,
            ));
        }
        let derived = grant_authority(tx, &current, input)?;
        let (id, token) = generate_managed_cloud_runtime_grant_credentials();
        let token_hash = managed_cloud_token_sha256(&token);
        for family in [
            MANAGED_CLOUD_BASE_RUNTIME_TABLES,
            MANAGED_CLOUD_ORIGINAL_SOURCE_VERIFIER_RUNTIME_TABLES,
        ] {
            if tx
                .optional(
                    &format!(
                        "SELECT 1 FROM {} WHERE grant_id=$1 OR token_sha256=$2",
                        family.grants
                    ),
                    &args![&id, &token_hash],
                )?
                .is_some()
            {
                return Err(ManagedCloudRegistryError::IdentityConflict);
            }
        }
        let expiry = now
            .checked_add(input.ttl_ms)
            .filter(|value| {
                *value <= derived.activation_expires_at_ms
                    && managed_cloud_safe_integer(*value, true)
            })
            .ok_or(ManagedCloudRegistryError::InvalidRequest)?;
        let ciphertext =
            encrypt_managed_cloud_runtime_grant_token(&id, &input.issuance_ref, &token)?;
        tx.execute(&format!("INSERT INTO {}(grant_id,token_sha256,grant_token_ciphertext,issuance_ref,
            environment,region,channel,activation_sha256,manifest_sha256,component_id,role,head_revision,transition_sha256,
            artifact_sha256,config_schema_sha256,migration_set_sha256,protocol_set_sha256,task_queue_sha256,
            failure_converter_sha256,expected_dependency_evidence_sha256,expected_runtime_identity_sha256,
            expected_worker_id,authorization_ref,created_by,activation_expires_at_ms,expires_at_ms,created_at_ms)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27)", tables.grants),
            &args![&id, &token_hash, ciphertext, &input.issuance_ref, &input.scope.environment, &input.scope.region,
                &input.scope.channel, &input.activation_sha256, &input.manifest_sha256, &input.component_id, &input.role,
                derived.head_revision, derived.transition_sha256, derived.artifact_sha256, derived.config_schema_sha256,
                derived.migration_set_sha256, derived.protocol_set_sha256, derived.task_queue_sha256,
                derived.failure_converter_sha256, derived.dependency_evidence_sha256, derived.runtime_identity_sha256,
                &input.expected_worker_id, &input.authorization_ref, &input.created_by, derived.activation_expires_at_ms, expiry, now])?;
        let (grant, _) = tx.grant(&id)?;
        Ok(managed_cloud_runtime_grant_result(
            grant,
            Some(&token),
            false,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::super::managed_cloud_release_authority_tests as legacy_tests;
    use super::*;

    fn fixture(pool: &DbPool) -> (NewManagedCloudRuntimeGrant, wire::AuthorityBinding) {
        managed_cloud_provisioning_persistence::tests::install_runtime_unit_fixture(pool)
    }
    fn sqlite() -> DbPool {
        super::super::tests::test_pool()
    }
    fn count(pool: &DbPool, table: &str) -> i64 {
        transaction(pool, |tx| {
            tx.one(&format!("SELECT COUNT(*) FROM {table}"), &[])?
                .integer(0)
        })
        .unwrap()
    }
    fn assert_guard_error(error: ManagedCloudRegistryError, allowed: &[&str]) {
        let ManagedCloudRegistryError::Storage(error) = error else {
            panic!("expected concrete SQL guard failure");
        };
        let message = if let Some(error) = error.downcast_ref::<rusqlite::Error>() {
            let rusqlite::Error::SqliteFailure(_, Some(message)) = error else {
                panic!("expected SQLite trigger refusal");
            };
            message.as_str()
        } else if let Some(error) = error.downcast_ref::<postgres::Error>() {
            error
                .as_db_error()
                .expect("expected PG trigger refusal")
                .message()
        } else {
            panic!("expected a concrete database guard");
        };
        assert!(
            allowed.contains(&message),
            "unexpected finite database guard"
        );
    }
    fn legacy_osv_partition_scenario(pool: &DbPool) {
        // Normal v1 APIs under the existing labelled synthetic legacy authority.
        // This is compatibility construction, not real fleet/startup qualification.
        let now = transaction(pool, |tx| tx.now()).unwrap();
        let scope = ManagedCloudScope {
            environment: "production".into(),
            region: "us-east-1".into(),
            channel: "general".into(),
        };
        let (activation, manifest) = legacy_tests::install_valid_original_source_managed_head(
            pool,
            3,
            now,
            30_000,
            600_000,
            scope.clone(),
            "phase648-legacy-partition-unit-only",
        );
        let grant = issue_managed_cloud_runtime_grant(
            pool,
            &NewManagedCloudRuntimeGrant {
                issuance_ref: "phase648-legacy-unit-issuance".into(),
                scope,
                activation_sha256: activation,
                manifest_sha256: manifest,
                component_id: "jobs-workflows".into(),
                role: "original_source_verifier".into(),
                expected_worker_id: "phase648-legacy-unit-worker".into(),
                authorization_ref: "phase648-legacy-unit-authorization".into(),
                created_by: "phase648-unit-only".into(),
                ttl_ms: 60_000,
            },
        )
        .unwrap();
        let token = grant.grant_token.unwrap();
        let runtime_id = "phase648-legacy-unit-runtime";
        let session =
            derive_managed_cloud_runtime_session_token(&token, &grant.grant_id, runtime_id)
                .unwrap();
        let instance = claim_managed_cloud_runtime_grant(
            pool,
            &ClaimManagedCloudRuntimeGrant {
                grant_id: grant.grant_id.clone(),
                grant_token: token.clone(),
                runtime_instance_id: runtime_id.into(),
                worker_id: grant.expected_worker_id.clone(),
                session_token: session.clone(),
                runtime_identity_sha256: grant.expected_runtime_identity_sha256.clone(),
            },
        )
        .unwrap();
        let registration = register_source_verifier_boot(
            pool,
            &SourceVerifierBootRegistrationRequest {
                session_token: session.clone(),
                registration_request_id: "phase648-legacy-unit-registration".into(),
                boot_id: "phase648-legacy-unit-boot".into(),
                runtime_instance_id: runtime_id.into(),
                instance_epoch: instance.instance_epoch,
                worker_id: instance.worker_id.clone(),
                runtime_identity_sha256: instance.runtime_identity_sha256.clone(),
                activation_sha256: instance.activation_sha256.clone(),
                manifest_sha256: instance.manifest_sha256.clone(),
                component_id: instance.component_id.clone(),
                role: instance.role.clone(),
            },
        )
        .unwrap();
        let mut heartbeat = ManagedCloudRuntimeHeartbeatInput {
            runtime_instance_id: runtime_id.into(),
            worker_id: instance.worker_id.clone(),
            session_token: session.clone(),
            heartbeat_sequence: 1,
            observed_head_revision: instance.head_revision,
            observed_transition_sha256: instance.transition_sha256.clone(),
            activation_sha256: instance.activation_sha256.clone(),
            manifest_sha256: instance.manifest_sha256.clone(),
            component_id: instance.component_id.clone(),
            role: instance.role.clone(),
            artifact_sha256: instance.artifact_sha256.clone(),
            migration_set_sha256: instance.migration_set_sha256.clone(),
            config_schema_sha256: instance.config_schema_sha256.clone(),
            protocol_set_sha256: instance.protocol_set_sha256.clone(),
            task_queue_sha256: instance.task_queue_sha256.clone(),
            failure_converter_sha256: instance.failure_converter_sha256.clone(),
            dependency_evidence_sha256: instance.dependency_evidence_sha256.clone(),
            health_state: "degraded".into(),
            reason_code: Some("startup".into()),
            boot_id: Some(registration.boot_id.clone()),
            boot_generation: Some(registration.boot_generation),
        };
        record_managed_cloud_runtime_heartbeat(pool, &heartbeat).unwrap();
        let startup = record_source_verifier_boot_startup(
            pool,
            &SourceVerifierBootStartupRequest {
                session_token: session,
                startup_request_id: "phase648-legacy-unit-startup".into(),
                boot_id: registration.boot_id.clone(),
                runtime_instance_id: runtime_id.into(),
                instance_epoch: instance.instance_epoch,
                boot_generation: registration.boot_generation,
                worker_id: instance.worker_id.clone(),
                degraded_heartbeat_sequence: 1,
            },
        )
        .unwrap();
        assert_eq!(startup.ready_heartbeat_sequence, 2);
        heartbeat.heartbeat_sequence = 3;
        heartbeat.reason_code = Some("probe_failed".into());
        assert!(
            record_managed_cloud_runtime_heartbeat(pool, &heartbeat).is_err(),
            "legacy same-boot Ready lowering remains denied"
        );
        let binding = wire::AuthorityBinding {
            authority_kind: wire::AuthorityKind::ProvisioningV4,
            purpose: wire::Purpose::Production,
            context_sha256: None,
            queue_generation_id: "phase648-unit-false-v4-generation".into(),
        };
        assert!(
            claim_provisioning_runtime_grant(
                pool,
                &ProvisioningProcessContext::production(),
                &wire::ClaimRequest {
                    version: 2,
                    authority_binding: binding.clone(),
                    grant_id: grant.grant_id.clone(),
                    grant_token: token.clone(),
                    runtime_instance_id: runtime_id.into(),
                    worker_id: instance.worker_id.clone(),
                    runtime_identity_sha256: instance.runtime_identity_sha256.clone(),
                    session_token: wire::derive_runtime_session_token_v2(
                        &token,
                        &grant.grant_id,
                        runtime_id,
                        &binding
                    )
                    .unwrap(),
                }
            )
            .is_err(),
            "v2 route cannot reinterpret a legacy activation/instance"
        );
        let mismatch = transaction(pool, |tx| {
            tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_boot_registrations(boot_id,registration_request_id,runtime_instance_id,
                instance_epoch,boot_generation,worker_id,runtime_identity_sha256,activation_sha256,manifest_sha256,component_id,role,
                registration_request_sha256,registration_authority_sha256,canonical_response_base64url,response_sha256,registered_at_ms,protocol_version)
                SELECT 'phase648-cross-version-boot','phase648-cross-version-registration',runtime_instance_id,instance_epoch,boot_generation,
                worker_id,runtime_identity_sha256,activation_sha256,manifest_sha256,component_id,role,
                registration_request_sha256,registration_authority_sha256,canonical_response_base64url,response_sha256,registered_at_ms,2
                FROM jobs_managed_cloud_source_verifier_boot_registrations WHERE boot_id=$1", &args![&registration.boot_id])?;
            Ok(())
        }).unwrap_err();
        assert_guard_error(
            mismatch,
            &["source-verifier boot registration protocol binding is invalid"],
        );
        let mismatch = transaction(pool, |tx| {
            let now = tx.now()?;
            tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_boot_startup_authorities(startup_request_id,boot_id,runtime_instance_id,
                instance_epoch,boot_generation,worker_id,request_sha256,degraded_heartbeat_sequence,ready_heartbeat_sequence,probe_result,
                startup_authority_sha256,canonical_response_base64url,response_sha256,startup_at_ms,protocol_version,
                prior_heartbeat_sequence,parked_heartbeat_sequence,dependency_result,observation_sha256)
                VALUES('phase648-cross-version-startup',$1,$2,$3,$4,$5,$6,NULL,NULL,NULL,NULL,'e30K',$7,$8,2,1,2,'dependency_ready',$9)",
                &args![&registration.boot_id, runtime_id, instance.instance_epoch, registration.boot_generation, &instance.worker_id,
                    "a".repeat(64), "b".repeat(64), now, "c".repeat(64)])?;
            Ok(())
        }).unwrap_err();
        assert_guard_error(
            mismatch,
            &[
                "source-verifier boot startup binding is invalid",
                "source-verifier parked startup receipt binding is invalid",
                "source-verifier boot startup requires current degraded heartbeat",
            ],
        );
        assert_eq!(
            count(
                pool,
                "jobs_managed_cloud_source_verifier_boot_registrations"
            ),
            1
        );
        assert_eq!(
            count(
                pool,
                "jobs_managed_cloud_source_verifier_boot_startup_authorities"
            ),
            1
        );
        assert_eq!(
            count(pool, "jobs_managed_cloud_osv_runtime_observations"),
            0
        );
    }
    fn role_input(input: &NewManagedCloudRuntimeGrant, role: &str) -> NewManagedCloudRuntimeGrant {
        let mut result = input.clone();
        result.role = role.to_owned();
        result.component_id = match role {
            "managed_runner" => "jobs-runner",
            "jobs_api" | "workflow_command_dispatcher" | "workflow_cleanup_dispatcher" => {
                "jobs-api"
            }
            _ => "jobs-workflows",
        }
        .to_owned();
        result.issuance_ref = format!("phase648-unit-issuance-{role}");
        result.expected_worker_id = format!("phase648-unit-worker-{role}");
        result
    }
    fn claim(
        pool: &DbPool,
        input: &NewManagedCloudRuntimeGrant,
        binding: &wire::AuthorityBinding,
    ) -> (wire::ClaimRequest, wire::ClaimResponse) {
        let process = ProvisioningProcessContext::production();
        let grant = issue_provisioning_runtime_grant(pool, &process, input).unwrap();
        let token = grant.grant_token.unwrap();
        let id = format!("phase648-unit-runtime-{}", input.role);
        let request = wire::ClaimRequest {
            version: 2,
            authority_binding: binding.clone(),
            grant_id: grant.grant_id.clone(),
            session_token: wire::derive_runtime_session_token_v2(
                &token,
                &grant.grant_id,
                &id,
                binding,
            )
            .unwrap(),
            grant_token: token,
            runtime_instance_id: id,
            worker_id: input.expected_worker_id.clone(),
            runtime_identity_sha256: grant.expected_runtime_identity_sha256,
        };
        let response = claim_provisioning_runtime_grant(pool, &process, &request).unwrap();
        (request, response)
    }
    fn probe(role: &str) -> wire::DependencyProbe {
        use wire::DependencyProbe as P;
        match role {
            "jobs_api" => P::JobsApi(wire::ApiProbe {
                version: 1,
                listener_state: wire::ListenerState::Listening,
                server_state: wire::ServerState::Running,
            }),
            "workflow_gateway" => P::WorkflowGateway(wire::GatewayProbe {
                version: 1,
                listener_state: wire::ListenerState::Listening,
                temporal_namespace: None,
            }),
            "workflow_worker" => P::WorkflowWorker(wire::WorkerProbe {
                version: 1,
                worker_state: wire::WorkerState::NotRunning,
                temporal_namespace: None,
            }),
            "workflow_command_dispatcher" => P::WorkflowCommandDispatcher(wire::DispatcherProbe {
                version: 1,
                loop_state: wire::LoopState::Parked,
                control_progress: wire::ControlProgress::Fresh,
                gateway_control: None,
            }),
            "workflow_cleanup_dispatcher" => P::WorkflowCleanupDispatcher(wire::DispatcherProbe {
                version: 1,
                loop_state: wire::LoopState::Parked,
                control_progress: wire::ControlProgress::Fresh,
                gateway_control: None,
            }),
            "discovery_worker" => P::DiscoveryWorker(wire::ControlProbe {
                version: 1,
                loop_state: wire::LoopState::Parked,
                control_progress: wire::ControlProgress::Fresh,
            }),
            "global_discovery_worker" => P::GlobalDiscoveryWorker(wire::ControlProbe {
                version: 1,
                loop_state: wire::LoopState::Parked,
                control_progress: wire::ControlProgress::Fresh,
            }),
            "original_source_verifier" => P::OriginalSourceVerifier(wire::SourceVerifierProbe {
                version: 1,
                loop_state: wire::LoopState::Parked,
                control_progress: wire::ControlProgress::Fresh,
                boot_startup: None,
            }),
            "managed_runner" => P::ManagedRunner(wire::RunnerProbe {
                version: 1,
                listener_state: wire::ListenerState::Closed,
                storage_recovery_complete: false,
                volume_binding: None,
            }),
            _ => panic!("closed synthetic role"),
        }
    }
    fn heartbeat(
        request: &wire::ClaimRequest,
        claimed: &wire::ClaimResponse,
    ) -> wire::HeartbeatRequest {
        wire::HeartbeatRequest {
            version: 2,
            authority_binding: claimed.authority_binding.clone(),
            runtime_instance_id: claimed.runtime_instance_id.clone(),
            worker_id: claimed.worker_id.clone(),
            session_token: request.session_token.clone(),
            heartbeat_sequence: 1,
            observed_head_revision: claimed.head_revision,
            observed_transition_sha256: claimed.transition_sha256.clone(),
            activation_sha256: claimed.activation_sha256.clone(),
            manifest_sha256: claimed.manifest_sha256.clone(),
            component_id: claimed.component_id.clone(),
            role: claimed.role.clone(),
            artifact_sha256: claimed.artifact_sha256.clone(),
            migration_set_sha256: claimed.migration_set_sha256.clone(),
            config_schema_sha256: claimed.config_schema_sha256.clone(),
            protocol_set_sha256: claimed.protocol_set_sha256.clone(),
            task_queue_sha256: claimed.task_queue_sha256.clone(),
            failure_converter_sha256: claimed.failure_converter_sha256.clone(),
            dependency_evidence_sha256: claimed.dependency_evidence_sha256.clone(),
            lifecycle_state: wire::LifecycleState::Parked,
            acknowledged_latch_sha256: None,
            dependency_probe: probe(&claimed.role),
            queue_observation: None,
            health_state: wire::HeartbeatHealthState::Degraded,
            reason_code: Some(wire::HeartbeatReasonCode::Startup),
            boot_id: None,
            boot_generation: None,
        }
    }
    fn register(
        pool: &DbPool,
        request: &wire::ClaimRequest,
        claimed: &wire::ClaimResponse,
    ) -> wire::BootRegistrationResponse {
        register_provisioning_source_verifier_boot(
            pool,
            &ProvisioningProcessContext::production(),
            &registration_request(request, claimed),
        )
        .unwrap()
    }
    fn registration_request(
        request: &wire::ClaimRequest,
        claimed: &wire::ClaimResponse,
    ) -> wire::BootRegistrationRequest {
        wire::BootRegistrationRequest {
            version: 2,
            authority_binding: claimed.authority_binding.clone(),
            session_token: request.session_token.clone(),
            registration_request_id: "phase648-unit-registration:/.-_".into(),
            boot_id: "phase648-unit-boot-identity:/.-_".into(),
            runtime_instance_id: claimed.runtime_instance_id.clone(),
            instance_epoch: claimed.instance_epoch,
            worker_id: claimed.worker_id.clone(),
            runtime_identity_sha256: claimed.runtime_identity_sha256.clone(),
            activation_sha256: claimed.activation_sha256.clone(),
            manifest_sha256: claimed.manifest_sha256.clone(),
            component_id: claimed.component_id.clone(),
            role: claimed.role.clone(),
        }
    }
    fn startup(
        request: &wire::ClaimRequest,
        claimed: &wire::ClaimResponse,
        registration: &wire::BootRegistrationResponse,
    ) -> wire::BootStartupRequest {
        wire::BootStartupRequest {
            version: 2,
            authority_binding: claimed.authority_binding.clone(),
            session_token: request.session_token.clone(),
            startup_request_id: "phase648-unit-startup-identity:/.-_".into(),
            boot_id: registration.boot_id.clone(),
            runtime_instance_id: claimed.runtime_instance_id.clone(),
            instance_epoch: claimed.instance_epoch,
            boot_generation: registration.boot_generation,
            worker_id: claimed.worker_id.clone(),
            prior_heartbeat_sequence: 1,
            dependency_probe: wire::DependencyProbe::OriginalSourceVerifier(
                wire::SourceVerifierProbe {
                    version: 1,
                    loop_state: wire::LoopState::Parked,
                    control_progress: wire::ControlProgress::Fresh,
                    boot_startup: Some(wire::BootStartup {
                        boot_id: registration.boot_id.clone(),
                        boot_generation: registration.boot_generation,
                        startup_request_id: "phase648-unit-startup-identity:/.-_".into(),
                        parked_heartbeat_sequence: 2,
                    }),
                },
            ),
        }
    }
    fn replay_scenario(
        pool: &DbPool,
        base: &NewManagedCloudRuntimeGrant,
        binding: &wire::AuthorityBinding,
    ) {
        let process = ProvisioningProcessContext::production();
        let input = role_input(base, "jobs_api");
        let (request, claimed) = claim(pool, &input, binding);
        assert!(!claimed.replayed && claimed.serving_latch_sha256.is_none());
        let mut hb = heartbeat(&request, &claimed);
        let first = record_provisioning_runtime_heartbeat(pool, &process, &hb).unwrap();
        let replay = record_provisioning_runtime_heartbeat(pool, &process, &hb).unwrap();
        assert!(replay.replayed);
        assert_eq!(first.heartbeat_at_ms, replay.heartbeat_at_ms);
        assert_eq!(first.observation_sha256, replay.observation_sha256);
        // Private selector only: the literal latch below is NOT accepted by the
        // real latch reader, and no Ready receipt/observation is generated.
        assert_eq!(
            select_unit_candidate(pool, &hb, &"6".repeat(64)),
            Some((1, first.observation_sha256.clone()))
        );
        hb.heartbeat_sequence = 2;
        hb.health_state = wire::HeartbeatHealthState::Draining;
        hb.reason_code = Some(wire::HeartbeatReasonCode::Draining);
        hb.dependency_probe = wire::DependencyProbe::JobsApi(wire::ApiProbe {
            version: 1,
            listener_state: wire::ListenerState::Closed,
            server_state: wire::ServerState::Stopped,
        });
        record_provisioning_runtime_heartbeat(pool, &process, &hb).unwrap();
        let negative_count = count(pool, "jobs_managed_cloud_runtime_observations");
        assert_eq!(
            select_unit_candidate(pool, &hb, &"6".repeat(64)),
            None,
            "latest negative breaks the chain; do not scan back to parked sequence one"
        );
        assert_eq!(
            count(pool, "jobs_managed_cloud_runtime_observations"),
            negative_count
        );
        transaction(pool, |tx| {
            let row = tx.one("SELECT heartbeat_sequence,health_state FROM jobs_managed_cloud_runtime_heartbeats WHERE runtime_instance_id=$1", &args![&claimed.runtime_instance_id])?;
            ensure(row.integer(0)? == 2 && row.text(1)? == "draining")
        }).unwrap();
        hb.heartbeat_sequence = 1;
        assert!(matches!(
            record_provisioning_runtime_heartbeat(pool, &process, &hb),
            Err(ManagedCloudRegistryError::HeartbeatSequenceConflict)
        ));
        let again = claim_provisioning_runtime_grant(pool, &process, &request).unwrap();
        assert!(again.replayed);
        assert_eq!(again.next_heartbeat_sequence, 3);
        assert_eq!(again.claimed_at_ms, claimed.claimed_at_ms);
        assert_eq!(again.serving_latch_sha256, claimed.serving_latch_sha256);
        assert_eq!(count(pool, "jobs_managed_cloud_runtime_observations"), 2);
        transaction(pool, |tx| {
            let row = tx.one("SELECT canonical_heartbeat_base64url FROM jobs_managed_cloud_runtime_observations WHERE runtime_instance_id=$1 AND heartbeat_sequence=1", &args![&claimed.runtime_instance_id])?;
            let value: serde_json::Value = decode(&row.text(0)?)?;
            ensure(value.get("sessionToken").is_none())
        }).unwrap();
    }
    fn osv_scenario(
        pool: &DbPool,
        base: &NewManagedCloudRuntimeGrant,
        binding: &wire::AuthorityBinding,
    ) {
        let process = ProvisioningProcessContext::production();
        let (request, claimed) =
            claim(pool, &role_input(base, "original_source_verifier"), binding);
        let registration = register(pool, &request, &claimed);
        let mut hb = heartbeat(&request, &claimed);
        hb.boot_id = Some(registration.boot_id.clone());
        hb.boot_generation = Some(registration.boot_generation);
        record_provisioning_runtime_heartbeat(pool, &process, &hb).unwrap();
        let mut start = startup(&request, &claimed, &registration);
        if let wire::DependencyProbe::OriginalSourceVerifier(probe) = &mut start.dependency_probe {
            probe.loop_state = wire::LoopState::Stopped;
            probe.control_progress = wire::ControlProgress::Stale;
        }
        assert!(matches!(
            record_provisioning_source_verifier_startup(pool, &process, &start),
            Err(ManagedCloudRegistryError::Unavailable)
        ));
        assert_eq!(
            count(
                pool,
                "jobs_managed_cloud_source_verifier_boot_startup_authorities"
            ),
            0
        );
        assert_eq!(
            count(pool, "jobs_managed_cloud_osv_runtime_observations"),
            1
        );
        start = startup(&request, &claimed, &registration); // Explicit manual corrected retry.
        let first = record_provisioning_source_verifier_startup(pool, &process, &start).unwrap();
        let replay = record_provisioning_source_verifier_startup(pool, &process, &start).unwrap();
        assert!(!first.replayed && !replay.replayed);
        assert_eq!(json(&first).unwrap(), json(&replay).unwrap());
        transaction(pool, |tx| {
            let now = tx.now()?;
            let auth = authenticated_instance(tx, &process, &start.runtime_instance_id, &start.worker_id,
                &start.session_token, &start.authority_binding, now)?;
            let row = startup_replay_row(tx, &start.startup_request_id)?.ok_or(ManagedCloudRegistryError::NotFound)?;
            ensure(json(&startup_replay_response(&row, &auth, &start)?)? == json(&first)?)?;
            let baseline = serde_json::to_value(&first).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
            for key in baseline.as_object().ok_or(ManagedCloudRegistryError::InvalidAuthority)?.keys() {
                let mut response = baseline.clone();
                let value = response.get_mut(key).ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
                *value = match value.clone() {
                    serde_json::Value::Bool(v) => serde_json::Value::Bool(!v),
                    serde_json::Value::Number(v) => serde_json::Value::from(v.as_i64().ok_or(ManagedCloudRegistryError::InvalidAuthority)? + 1),
                    serde_json::Value::String(v) => serde_json::Value::String(match key.as_str() {
                        "lifecycleState" => "operational".into(), "healthState" => "ready".into(),
                        "role" => "workflow_worker".into(), "componentId" => "jobs-api".into(),
                        _ if v.len() == 64 => if v == "f".repeat(64) { "e".repeat(64) } else { "f".repeat(64) },
                        _ => "phase648-unit-rebound-identity".into(),
                    }),
                    serde_json::Value::Null => match key.as_str() {
                        "reasonCode" => serde_json::Value::String("dependency_unavailable".into()),
                        "queueObservation" => serde_json::json!({"pollerIdentitySha256":"a".repeat(64),"backlogCount":0,"deliveredTaskCount":0}),
                        _ => serde_json::Value::String("f".repeat(64)),
                    },
                    serde_json::Value::Object(mut v) => {
                        if key == "authorityBinding" { v.insert("queueGenerationId".into(), "phase648-unit-rebound-generation".into()); }
                        else { v.insert("controlProgress".into(), "stale".into()); }
                        serde_json::Value::Object(v)
                    }
                    _ => return Err(ManagedCloudRegistryError::InvalidAuthority),
                };
                let mut rebound = Record(row.0.clone());
                rebound.0[1] = Value::Text(encode(&response)?);
                rebound.0[2] = Value::Text(boot_digest(BOOT_STARTUP_RESPONSE_DOMAIN, &response, false)?);
                ensure(startup_replay_response(&rebound, &auth, &start).is_err())?;
            }
            for index in [5, 6, 9, 11, 12, 14, 19, 21] {
                let mut rebound = Record(row.0.clone());
                rebound.0[index] = Value::Integer(row.integer(index)? + 1);
                ensure(startup_replay_response(&rebound, &auth, &start).is_err())?;
            }
            for index in [0, 2, 7, 8, 10, 15, 16, 17, 18, 20, 22, 23] {
                let mut rebound = Record(row.0.clone());
                let value = if row.optional_text(index)? == Some("f".repeat(64)) { "e".repeat(64) } else { "f".repeat(64) };
                rebound.0[index] = Value::Text(value);
                ensure(startup_replay_response(&rebound, &auth, &start).is_err())?;
            }
            // Read the actual stored probe bytes, not a reconstructed encoding
            // of the request. Canonical decoding and structural binding both
            // remain necessary even with an internally consistent HB receipt.
            let probe_bytes = wire::canonical_probe_bytes(&start.dependency_probe)?;
            let mut extra_lf = probe_bytes.clone();
            extra_lf.push(b'\n');
            let duplicate = String::from_utf8(probe_bytes.clone())
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?
                .replacen("\"version\":1", "\"version\":1,\"version\":1", 1).into_bytes();
            let mut other = serde_json::to_value(&start.dependency_probe)
                .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
            other["controlProgress"] = serde_json::Value::String("stale".into());
            for bytes in [extra_lf, duplicate, vec![0xff], vec![b' '; 64 * 1024 + 1], managed_cloud_canonical_json(&other)?] {
                let mut rebound = Record(row.0.clone());
                rebound.0[17] = Value::Text(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes));
                ensure(startup_replay_response(&rebound, &auth, &start).is_err())?;
            }
            Ok(())
        }).unwrap();
        assert_eq!(first.heartbeat_sequence, 2);
        assert_eq!(first.lifecycle_state, wire::LifecycleState::Parked);
        assert_eq!(first.health_state, wire::HeartbeatHealthState::Degraded);
        assert_eq!(
            count(pool, "jobs_managed_cloud_osv_runtime_observations"),
            2
        );
        assert_eq!(count(pool, "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings"), 0,
            "v2 uses its own permanent observation -> protocol2 registration, never a legacy boot proof");
        let cross_binding = transaction(pool, |tx| {
            tx.execute("INSERT INTO jobs_managed_cloud_source_verifier_heartbeat_boot_bindings(runtime_instance_id,instance_epoch,heartbeat_sequence,boot_id,boot_generation)
                VALUES($1,$2,$3,$4,$5)", &args![&claimed.runtime_instance_id, claimed.instance_epoch, first.heartbeat_sequence,
                    &registration.boot_id, registration.boot_generation])?;
            Ok(())
        }).unwrap_err();
        assert_guard_error(
            cross_binding,
            &["source-verifier heartbeat boot binding is stale"],
        );
        let instance =
            transaction(pool, |tx| Ok(tx.instance(&claimed.runtime_instance_id)?.0)).unwrap();
        let binding = OriginalSourceVerifierBinding {
            worker_id: claimed.worker_id.clone(),
            runtime_instance_id: claimed.runtime_instance_id.clone(),
            runtime_instance_epoch: claimed.instance_epoch,
            runtime_authority_sha256: original_source_verifier_runtime_authority_sha256(&instance)
                .unwrap(),
            runtime_session_token: request.session_token.clone(),
            boot_id: Some(registration.boot_id.clone()),
            boot_generation: Some(registration.boot_generation),
        };
        // Both refusals are read-only. Keep the persisted wide v2 boot identity
        // intact: its legacy shape failure and the v4 authority fence are
        // separate predicates, not interchangeable successful fence evidence.
        let snapshot = || {
            transaction(pool, |tx| {
                [
                    "jobs_managed_cloud_osv_runtime_observations",
                    "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
                    "jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit",
                    "jobs_managed_cloud_source_verifier_boot_generation_heads",
                    "jobs_original_source_verification_assignments",
                    "jobs_original_source_verification_attempts",
                    "jobs_original_source_verification_events",
                    "jobs_original_source_verification_receipts",
                    "jobs_original_source_verification_heads",
                    "jobs_original_source_verification_transitions",
                    "jobs_managed_cloud_heads",
                ]
                .iter()
                .map(|table| {
                    Ok(tx
                        .rows(&format!("SELECT * FROM {table} ORDER BY 1,2,3"), &[])?
                        .into_iter()
                        .map(|row| row.0)
                        .collect::<Vec<_>>())
                })
                .collect::<ManagedCloudResult<Vec<_>>>()
            })
            .unwrap()
        };
        let before = snapshot();
        assert!(matches!(
            lease_original_source_verification(pool, &binding),
            Err(OriginalSourceVerificationError::InvalidInput(message))
                if message == "managed verifier binding is invalid"
        ));
        assert_eq!(before, snapshot());
        let legacy_shaped_binding = OriginalSourceVerifierBinding {
            boot_id: Some("phase648-unit-legacy-shaped-boot".into()),
            ..binding.clone()
        };
        assert!(matches!(
            lease_original_source_verification(pool, &legacy_shaped_binding),
            Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
        ));
        assert_eq!(before, snapshot());
        transaction(pool, |tx| {
            let row = tx.one("SELECT ready_boot_generation FROM jobs_managed_cloud_source_verifier_boot_generation_heads WHERE runtime_instance_id=$1 AND instance_epoch=$2", &args![&claimed.runtime_instance_id, claimed.instance_epoch])?;
            ensure(matches!(row.0.first(), Some(Value::Null)))
        }).unwrap();
        start.prior_heartbeat_sequence = 2;
        assert!(matches!(
            record_provisioning_source_verifier_startup(pool, &process, &start),
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
    }

    #[test]
    fn provisioning_runtime_sqlite_claim_and_heartbeat_replay_preserve_original_facts() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (input, binding) = fixture(&pool);
        replay_scenario(&pool, &input, &binding);
    }

    #[test]
    fn provisioning_runtime_sqlite_osv_startup_is_atomic_parked_and_replay_is_immutable() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (input, binding) = fixture(&pool);
        osv_scenario(&pool, &input, &binding);
    }

    #[test]
    fn provisioning_runtime_sqlite_all_nine_roles_can_report_truthful_nonserving_health() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        for role in [
            "jobs_api",
            "workflow_gateway",
            "workflow_worker",
            "workflow_command_dispatcher",
            "workflow_cleanup_dispatcher",
            "discovery_worker",
            "global_discovery_worker",
            "original_source_verifier",
            "managed_runner",
        ] {
            let (request, claimed) = claim(&pool, &role_input(&base, role), &binding);
            let mut hb = heartbeat(&request, &claimed);
            if role == "original_source_verifier" {
                let registration = register(&pool, &request, &claimed);
                hb.boot_id = Some(registration.boot_id);
                hb.boot_generation = Some(registration.boot_generation);
            }
            let result = record_provisioning_runtime_heartbeat(
                &pool,
                &ProvisioningProcessContext::production(),
                &hb,
            )
            .unwrap();
            assert_eq!(result.health_state, wire::HeartbeatHealthState::Degraded);
            assert!(result.serving_latch_sha256.is_none());
        }
        let readiness = resolve_managed_cloud_readiness(
            &pool,
            &ManagedCloudReadinessQuery {
                scope: base.scope,
                account_id: None,
            },
        )
        .unwrap();
        assert!(!readiness.status.authority_ready && !readiness.status.customer_admission);
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_observations"), 8);
        assert_eq!(
            count(&pool, "jobs_managed_cloud_osv_runtime_observations"),
            1
        );
    }

    #[test]
    fn provisioning_runtime_sqlite_wrong_purpose_session_identity_and_sequence_do_not_write() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (mut request, claimed) = claim(&pool, &base, &binding);
        request.authority_binding.purpose = wire::Purpose::NonproductionSynthetic;
        request.authority_binding.context_sha256 = Some("a".repeat(64));
        assert!(claim_provisioning_runtime_grant(
            &pool,
            &ProvisioningProcessContext::production(),
            &request
        )
        .is_err());
        request.authority_binding = binding;
        let mut wrong_session = request.clone();
        wrong_session.session_token = derive_managed_cloud_runtime_session_token(
            &request.grant_token,
            &request.grant_id,
            &request.runtime_instance_id,
        )
        .unwrap();
        assert!(
            claim_provisioning_runtime_grant(
                &pool,
                &ProvisioningProcessContext::production(),
                &wrong_session
            )
            .is_err(),
            "v2 does not fall back to a legacy session derivation"
        );
        let mut hb = heartbeat(&request, &claimed);
        hb.session_token = "A".repeat(43);
        assert!(record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb
        )
        .is_err());
        hb.session_token = request.session_token;
        hb.artifact_sha256 = "f".repeat(64);
        assert!(record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb
        )
        .is_err());
        hb.artifact_sha256 = claimed.artifact_sha256;
        hb.heartbeat_sequence = 2;
        assert!(record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb
        )
        .is_err());
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_observations"), 0);
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_heartbeats"), 0);
    }

    #[test]
    fn provisioning_runtime_sqlite_failed_latest_write_rolls_back_permanent_observation_and_audit()
    {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (request, claimed) = claim(&pool, &base, &binding);
        pool.get().unwrap().execute_batch("CREATE TRIGGER phase648_unit_latest_failure BEFORE INSERT ON jobs_managed_cloud_runtime_heartbeats BEGIN SELECT RAISE(ABORT,'phase648 unit injected latest failure'); END;").unwrap();
        assert!(record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &heartbeat(&request, &claimed)
        )
        .is_err());
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_observations"), 0);
        assert_eq!(
            count(&pool, "jobs_managed_cloud_runtime_heartbeat_audit"),
            0
        );
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_heartbeats"), 0);
    }

    #[test]
    fn provisioning_runtime_sqlite_unexpected_worker_delivery_is_sticky_and_replay_safe() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (request, claimed) = claim(&pool, &role_input(&base, "workflow_worker"), &binding);
        let mut hb = heartbeat(&request, &claimed);
        hb.queue_observation = Some(wire::QueueObservation {
            poller_identity_sha256: "a".repeat(64),
            backlog_count: 0,
            delivered_task_count: 1,
        });
        record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb,
        )
        .unwrap();
        assert!(
            record_provisioning_runtime_heartbeat(
                &pool,
                &ProvisioningProcessContext::production(),
                &hb
            )
            .unwrap()
            .replayed
        );
        assert_eq!(count(&pool, "jobs_managed_cloud_queue_contaminations"), 1);
        hb.heartbeat_sequence = 2;
        hb.queue_observation.as_mut().unwrap().delivered_task_count = 0;
        record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb,
        )
        .unwrap();
        assert_eq!(count(&pool, "jobs_managed_cloud_queue_contaminations"), 1);
    }

    #[test]
    fn provisioning_runtime_sqlite_revoked_grant_cannot_replay_or_renew_health() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (request, claimed) = claim(&pool, &base, &binding);
        let hb = heartbeat(&request, &claimed);
        record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb,
        )
        .unwrap();
        revoke_managed_cloud_runtime_grant(
            &pool,
            &RevokeManagedCloudRuntimeGrant {
                grant_id: request.grant_id.clone(),
                reason_ref: "phase648-unit-grant-revocation".into(),
                revoked_by: "phase648-unit-only".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            record_provisioning_runtime_heartbeat(
                &pool,
                &ProvisioningProcessContext::production(),
                &hb
            ),
            Err(ManagedCloudRegistryError::Revoked)
        ));
        assert!(claim_provisioning_runtime_grant(
            &pool,
            &ProvisioningProcessContext::production(),
            &request
        )
        .is_err());
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_observations"), 1);
    }

    /// Exercises only the transaction-local predecessor selector using declared
    /// unit metadata. It never calls the real closed latch reader or writes a
    /// Ready observation/receipt and is not runtime qualification.
    fn select_unit_candidate(
        pool: &DbPool,
        previous: &wire::HeartbeatRequest,
        latch: &str,
    ) -> Option<(i64, String)> {
        let mut candidate = previous.clone();
        candidate.heartbeat_sequence += 1;
        candidate.lifecycle_state = wire::LifecycleState::Operational;
        candidate.health_state = wire::HeartbeatHealthState::Ready;
        candidate.reason_code = None;
        candidate.acknowledged_latch_sha256 = Some(latch.to_owned());
        match &mut candidate.dependency_probe {
            wire::DependencyProbe::JobsApi(probe) => {
                probe.listener_state = wire::ListenerState::Listening;
                probe.server_state = wire::ServerState::Running;
            }
            wire::DependencyProbe::OriginalSourceVerifier(probe) => {
                probe.loop_state = wire::LoopState::Running;
                probe.control_progress = wire::ControlProgress::Fresh;
            }
            _ => panic!("closed unit selector cases"),
        }
        transaction(pool, |tx| {
            let now = tx.now()?;
            let auth = authenticated_instance(
                tx,
                &ProvisioningProcessContext::production(),
                &candidate.runtime_instance_id,
                &candidate.worker_id,
                &candidate.session_token,
                &candidate.authority_binding,
                now,
            )?;
            ensure(positive_probe(
                tx,
                &auth,
                &candidate,
                Some(latch),
                now,
                None,
            )?)?;
            parked_predecessor(tx, &auth, &candidate, Some(latch), now)
        })
        .unwrap()
    }

    fn concurrent_replay_scenario(
        pool: &DbPool,
        base: &NewManagedCloudRuntimeGrant,
        binding: &wire::AuthorityBinding,
    ) {
        let (request, claimed) = claim(pool, &role_input(base, "discovery_worker"), binding);
        let hb = heartbeat(&request, &claimed);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let hb = hb.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    record_provisioning_runtime_heartbeat(
                        &pool,
                        &ProvisioningProcessContext::production(),
                        &hb,
                    )
                })
            })
            .collect::<Vec<_>>();
        let responses = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_ne!(responses[0].replayed, responses[1].replayed);
        assert_eq!(
            responses[0].observation_sha256,
            responses[1].observation_sha256
        );
        assert_eq!(responses[0].heartbeat_at_ms, responses[1].heartbeat_at_ms);
        transaction(pool, |tx| {
            ensure(tx.one("SELECT COUNT(*) FROM jobs_managed_cloud_runtime_observations WHERE runtime_instance_id=$1", &args![&claimed.runtime_instance_id])?.integer(0)? == 1)
        }).unwrap();
    }

    /// DDL regression ONLY. This intentionally unqualified metadata fixture
    /// cannot pass current_latch: its document is labelled unit-only, not the
    /// future signed-latch wire. It never grants customer/business authority.
    fn storage_only_osv_lowering_scenario(
        pool: &DbPool,
        base: &NewManagedCloudRuntimeGrant,
        binding: &wire::AuthorityBinding,
    ) {
        let mut osv = None;
        for role in [
            "jobs_api",
            "workflow_gateway",
            "workflow_worker",
            "workflow_command_dispatcher",
            "workflow_cleanup_dispatcher",
            "discovery_worker",
            "global_discovery_worker",
            "original_source_verifier",
            "managed_runner",
        ] {
            let (request, claimed) = claim(pool, &role_input(base, role), binding);
            let mut hb = heartbeat(&request, &claimed);
            if role == "original_source_verifier" {
                let registration = register(pool, &request, &claimed);
                hb.boot_id = Some(registration.boot_id.clone());
                hb.boot_generation = Some(registration.boot_generation);
                record_provisioning_runtime_heartbeat(
                    pool,
                    &ProvisioningProcessContext::production(),
                    &hb,
                )
                .unwrap();
                let start = startup(&request, &claimed, &registration);
                let response = record_provisioning_source_verifier_startup(
                    pool,
                    &ProvisioningProcessContext::production(),
                    &start,
                )
                .unwrap();
                hb.heartbeat_sequence = 2;
                hb.dependency_probe = start.dependency_probe;
                osv = Some((claimed, hb, response));
            } else {
                record_provisioning_runtime_heartbeat(
                    pool,
                    &ProvisioningProcessContext::production(),
                    &hb,
                )
                .unwrap();
            }
        }
        let (claimed, mut hb, parked) = osv.unwrap();
        let bootstrap_document =
            serde_json::json!({ "version": 1, "unitOnlyUnqualifiedStorageFixture": true });
        let bootstrap_sha =
            managed_cloud_sha256(&managed_cloud_canonical_json(&bootstrap_document).unwrap());
        let latch_document = serde_json::json!({ "version": 1, "unitOnlyUnqualifiedStorageFixture": true, "bootstrapSetSha256": bootstrap_sha.clone() });
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &latch_document,
            managed_cloud_provisioning_persistence::LATCH_AUDIENCE,
            "general_promotion",
            parked.heartbeat_at_ms,
        );
        let latch_bytes =
            managed_cloud_decode_base64url(&signed.envelope.canonical_base64url).unwrap();
        let latch_sha = managed_cloud_sha256(&latch_bytes);
        let signatures: ManagedCloudSignatureSetAuthority =
            decode(&signed.envelope.signature_set_base64url).unwrap();
        let signature_sha = managed_cloud_sha256(
            &managed_cloud_decode_base64url(&signed.envelope.signature_set_base64url).unwrap(),
        );
        transaction(pool, |tx| {
            let now = tx.now()?;
            match tx {
                ControlTransaction::Sqlite(tx) => insert_sqlite_managed_cloud_signature_set(tx, &signatures, &signature_sha,
                    &signed.envelope.signature_set_base64url, "phase648-unit-storage-only", now)?,
                ControlTransaction::Postgres(tx) => insert_postgres_managed_cloud_signature_set(tx, &signatures, &signature_sha,
                    &signed.envelope.signature_set_base64url, "phase648-unit-storage-only", now)?,
            }
            tx.execute("INSERT INTO jobs_managed_cloud_bootstrap_sets(bootstrap_set_sha256,activation_sha256,manifest_sha256,
                environment,region,channel,authority_kind,purpose,context_sha256,queue_generation_id,task_queue_sha256,
                head_revision,transition_sha256,trust_generation,channel_sequence,member_count,canonical_bootstrap_base64url,recorded_at_ms)
                SELECT $1,a.activation_sha256,a.manifest_sha256,a.environment,a.region,a.channel,a.authority_kind,a.purpose,
                a.context_sha256,a.queue_generation_id,a.task_queue_sha256,h.head_revision,h.current_transition_sha256,
                a.trust_generation,a.channel_sequence,9,$2,$3 FROM jobs_managed_cloud_activations a
                JOIN jobs_managed_cloud_heads h ON h.current_activation_sha256=a.activation_sha256 WHERE a.activation_sha256=$4",
                &args![&bootstrap_sha, encode(&bootstrap_document)?, now, &claimed.activation_sha256])?;
            for (members, latest, observations) in [
                ("jobs_managed_cloud_bootstrap_runtime_members", "jobs_managed_cloud_runtime_heartbeats", "jobs_managed_cloud_runtime_observations"),
                ("jobs_managed_cloud_bootstrap_source_verifier_members", "jobs_managed_cloud_original_source_verifier_runtime_heartbeats", "jobs_managed_cloud_osv_runtime_observations"),
            ] {
                tx.execute(&format!("INSERT INTO {members}(bootstrap_set_sha256,runtime_instance_id,instance_epoch,heartbeat_sequence,
                    observation_sha256,activation_sha256,manifest_sha256,head_revision,transition_sha256)
                    SELECT $1,o.runtime_instance_id,o.instance_epoch,o.heartbeat_sequence,o.observation_sha256,o.activation_sha256,
                    o.manifest_sha256,o.head_revision,o.transition_sha256 FROM {latest} h JOIN {observations} o
                      ON o.observation_sha256=h.observation_sha256 WHERE o.activation_sha256=$2"),
                    &args![&bootstrap_sha, &claimed.activation_sha256])?;
            }
            tx.execute("INSERT INTO jobs_managed_cloud_serving_latches(latch_sha256,latch_id,bootstrap_set_sha256,activation_sha256,
                manifest_sha256,environment,region,channel,authority_kind,purpose,context_sha256,queue_generation_id,task_queue_sha256,
                head_revision,transition_sha256,trust_generation,channel_sequence,authorization_signature_set_sha256,
                canonical_latch_base64url,issued_at_ms,not_before_ms,expires_at_ms,recorded_at_ms)
                SELECT $1,'phase648-unit-storage-only-latch',$2,a.activation_sha256,a.manifest_sha256,a.environment,a.region,
                a.channel,a.authority_kind,a.purpose,a.context_sha256,a.queue_generation_id,a.task_queue_sha256,h.head_revision,
                h.current_transition_sha256,a.trust_generation,a.channel_sequence,$3,$4,$5,$5,a.expires_at_ms,$6
                FROM jobs_managed_cloud_activations a JOIN jobs_managed_cloud_heads h ON h.current_activation_sha256=a.activation_sha256
                WHERE a.activation_sha256=$7", &args![&latch_sha, &bootstrap_sha, &signature_sha,
                    &signed.envelope.canonical_base64url, parked.heartbeat_at_ms, now, &claimed.activation_sha256])?;
            Ok(())
        }).unwrap();
        // Real writer rejects this unqualified latch metadata, despite actual
        // row signatures/FKs: this fixture cannot accidentally become authority.
        hb.heartbeat_sequence = 3;
        assert!(record_provisioning_runtime_heartbeat(
            pool,
            &ProvisioningProcessContext::production(),
            &hb
        )
        .is_err());
        hb.lifecycle_state = wire::LifecycleState::Operational;
        hb.acknowledged_latch_sha256 = Some(latch_sha.clone());
        if let wire::DependencyProbe::OriginalSourceVerifier(probe) = &mut hb.dependency_probe {
            probe.loop_state = wire::LoopState::Running;
        }
        for (sequence, health, reason) in [
            (3, wire::HeartbeatHealthState::Ready, None),
            (4, wire::HeartbeatHealthState::Ready, None),
            (
                5,
                wire::HeartbeatHealthState::Degraded,
                Some(wire::HeartbeatReasonCode::ProbeFailed),
            ),
            (
                6,
                wire::HeartbeatHealthState::Draining,
                Some(wire::HeartbeatReasonCode::Draining),
            ),
        ] {
            hb.heartbeat_sequence = sequence;
            hb.health_state = health;
            hb.reason_code = reason;
            if health != wire::HeartbeatHealthState::Ready {
                if let wire::DependencyProbe::OriginalSourceVerifier(probe) =
                    &mut hb.dependency_probe
                {
                    probe.loop_state = wire::LoopState::Stopped;
                    probe.control_progress = wire::ControlProgress::Stale;
                }
            }
            transaction(pool, |tx| {
                let now = tx.now()?;
                let digest = wire::observation_sha256_v2(&claimed.grant_id, &hb, now, claimed.instance_epoch, Some(&latch_sha))?;
                let phase = enum_text(&hb.lifecycle_state)?; let health = enum_text(&hb.health_state)?;
                let reason = hb.reason_code.map(|v| enum_text(&v)).transpose()?;
                let mut columns = OBSERVATION_COLUMNS.split(',').map(str::trim).collect::<Vec<_>>();
                columns.extend(["boot_id", "boot_generation"]);
                let replacements = [
                    ("heartbeat_sequence", "$1"), ("observation_sha256", "$2"), ("lifecycle_state", "$3"),
                    ("health_state", "$4"), ("reason_code", "$5"), ("acknowledged_latch_sha256", "$6"),
                    ("serving_latch_sha256", "$6"), ("canonical_heartbeat_base64url", "$7"),
                    ("dependency_probe_base64url", "$8"), ("parked_predecessor_sequence", "$9"),
                    ("parked_predecessor_sha256", "$10"), ("heartbeat_at_ms", "$11"),
                ];
                let projection = columns.iter().map(|column| replacements.iter().find(|(name, _)| name == column)
                    .map(|(_, value)| (*value).to_owned()).unwrap_or_else(|| format!("o.{column}"))).collect::<Vec<_>>().join(",");
                tx.execute(&format!("INSERT INTO jobs_managed_cloud_osv_runtime_observations({})
                    SELECT {projection} FROM jobs_managed_cloud_osv_runtime_observations o
                    WHERE o.runtime_instance_id=$12 AND o.instance_epoch=$13 AND o.heartbeat_sequence=2", columns.join(",")),
                    &args![sequence, &digest, phase, &health, reason.as_deref(), &latch_sha,
                        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonsecret_heartbeat(&hb)?),
                        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(wire::canonical_probe_bytes(&hb.dependency_probe)?),
                        if hb.health_state == wire::HeartbeatHealthState::Ready { Some(2_i64) } else { None },
                        if hb.health_state == wire::HeartbeatHealthState::Ready { Some(parked.observation_sha256.as_str()) } else { None }, now,
                        &claimed.runtime_instance_id, claimed.instance_epoch])?;
                ensure(tx.execute("UPDATE jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                    SET heartbeat_sequence=$3,health_state=$4,reason_code=$5,heartbeat_at_ms=$6,observation_sha256=$7
                    WHERE runtime_instance_id=$1 AND instance_epoch=$2 AND heartbeat_sequence=$8",
                    &args![&claimed.runtime_instance_id, claimed.instance_epoch, sequence, &health, reason.as_deref(), now, &digest, sequence - 1])? == 1)?;
                if sequence == 3 {
                    tx.execute("UPDATE jobs_managed_cloud_source_verifier_boot_generation_heads SET ready_boot_generation=current_boot_generation,
                        updated_at_ms=$3 WHERE runtime_instance_id=$1 AND instance_epoch=$2", &args![&claimed.runtime_instance_id, claimed.instance_epoch, now])?;
                }
                Ok(())
            }).unwrap();
            let selected = select_unit_candidate(pool, &hb, &latch_sha);
            if health == wire::HeartbeatHealthState::Ready {
                assert_eq!(
                    selected,
                    Some((2, parked.observation_sha256.clone())),
                    "exact-latch operational intermediates preserve the SAME parked tuple"
                );
            } else {
                assert_eq!(
                    selected, None,
                    "negative latest breaks even an inherited operational chain"
                );
            }
        }
        transaction(pool, |tx| {
            let row = tx.one("SELECT health_state,heartbeat_sequence FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                WHERE runtime_instance_id=$1", &args![&claimed.runtime_instance_id])?;
            ensure(row.text(0)? == "draining" && row.integer(1)? == 6)
        }).unwrap();
        assert_eq!(
            count(
                pool,
                "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings"
            ),
            0
        );
        let readiness = resolve_managed_cloud_readiness(
            pool,
            &ManagedCloudReadinessQuery {
                scope: base.scope.clone(),
                account_id: None,
            },
        )
        .unwrap();
        assert!(!readiness.status.customer_admission && !readiness.status.authority_ready);
    }

    #[test]
    fn provisioning_runtime_sqlite_storage_only_v2_ready_can_lower_without_legacy_boot_proof() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        storage_only_osv_lowering_scenario(&pool, &base, &binding);
    }

    #[test]
    fn provisioning_runtime_sqlite_concurrent_exact_heartbeat_has_one_immutable_commit() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        concurrent_replay_scenario(&pool, &base, &binding);
    }

    #[test]
    fn provisioning_runtime_sqlite_v1_routes_cannot_replay_v2_instances() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (request, claimed) = claim(&pool, &base, &binding);
        let hb = heartbeat(&request, &claimed);
        record_provisioning_runtime_heartbeat(
            &pool,
            &ProvisioningProcessContext::production(),
            &hb,
        )
        .unwrap();
        assert!(claim_managed_cloud_runtime_grant(
            &pool,
            &ClaimManagedCloudRuntimeGrant {
                grant_id: request.grant_id,
                grant_token: request.grant_token,
                runtime_instance_id: request.runtime_instance_id,
                worker_id: request.worker_id,
                session_token: request.session_token.clone(),
                runtime_identity_sha256: request.runtime_identity_sha256
            }
        )
        .is_err());
        assert!(record_managed_cloud_runtime_heartbeat(
            &pool,
            &ManagedCloudRuntimeHeartbeatInput {
                runtime_instance_id: hb.runtime_instance_id,
                worker_id: hb.worker_id,
                session_token: request.session_token,
                heartbeat_sequence: hb.heartbeat_sequence,
                observed_head_revision: hb.observed_head_revision,
                observed_transition_sha256: hb.observed_transition_sha256,
                activation_sha256: hb.activation_sha256,
                manifest_sha256: hb.manifest_sha256,
                component_id: hb.component_id,
                role: hb.role,
                artifact_sha256: hb.artifact_sha256,
                migration_set_sha256: hb.migration_set_sha256,
                config_schema_sha256: hb.config_schema_sha256,
                protocol_set_sha256: hb.protocol_set_sha256,
                task_queue_sha256: hb.task_queue_sha256,
                failure_converter_sha256: hb.failure_converter_sha256,
                dependency_evidence_sha256: hb.dependency_evidence_sha256,
                health_state: "degraded".into(),
                reason_code: Some("startup".into()),
                boot_id: None,
                boot_generation: None
            }
        )
        .is_err());
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_observations"), 1);
    }

    #[test]
    fn provisioning_runtime_sqlite_legacy_boot_stays_v1_and_denies_cross_version_writes() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        legacy_osv_partition_scenario(&sqlite());
    }

    #[test]
    fn provisioning_runtime_process_context_has_no_synthetic_or_blank_startup_fallback() {
        assert!(ProvisioningProcessContext::from_startup_value(None).is_ok());
        assert!(ProvisioningProcessContext::from_startup_value(Some("production".into())).is_ok());
        for value in [
            "",
            " production",
            "production\n",
            "true",
            "nonproduction_synthetic",
            "synthetic",
        ] {
            assert!(ProvisioningProcessContext::from_startup_value(Some(value.into())).is_err());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            assert!(ProvisioningProcessContext::from_startup_value(Some(
                std::ffi::OsString::from_vec(vec![0xff])
            ))
            .is_err());
        }
    }

    #[test]
    fn provisioning_runtime_finite_health_requires_exact_phase_latch_and_reason_pairs() {
        let binding = wire::AuthorityBinding {
            authority_kind: wire::AuthorityKind::ProvisioningV4,
            purpose: wire::Purpose::Production,
            context_sha256: None,
            queue_generation_id: "q".repeat(20),
        };
        let request = wire::ClaimRequest {
            version: 2,
            authority_binding: binding.clone(),
            grant_id: "g".repeat(20),
            grant_token: "A".repeat(43),
            runtime_instance_id: "r".repeat(20),
            worker_id: "w".repeat(20),
            session_token: "A".repeat(43),
            runtime_identity_sha256: "a".repeat(64),
        };
        let instance = ManagedCloudRuntimeInstance {
            grant_id: request.grant_id.clone(),
            runtime_instance_id: request.runtime_instance_id.clone(),
            runtime_identity_sha256: request.runtime_identity_sha256.clone(),
            worker_id: request.worker_id.clone(),
            scope: ManagedCloudScope {
                environment: "production".into(),
                region: "us-east-1".into(),
                channel: "general".into(),
            },
            activation_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            component_id: "jobs-api".into(),
            role: "jobs_api".into(),
            head_revision: 1,
            transition_sha256: "c".repeat(64),
            artifact_sha256: "d".repeat(64),
            config_schema_sha256: "e".repeat(64),
            migration_set_sha256: "f".repeat(64),
            protocol_set_sha256: "0".repeat(64),
            task_queue_sha256: "1".repeat(64),
            failure_converter_sha256: "2".repeat(64),
            dependency_evidence_sha256: "3".repeat(64),
            activation_expires_at_ms: 1000,
            instance_epoch: 1,
            next_heartbeat_sequence: 1,
            claimed_at_ms: 1,
            replayed: false,
        };
        let mut hb = heartbeat(&request, &claim_response(instance, binding, None));
        assert!(finite_health(&hb, None).is_ok());
        hb.health_state = wire::HeartbeatHealthState::Ready;
        hb.reason_code = None;
        assert!(finite_health(&hb, None).is_err());
        hb.lifecycle_state = wire::LifecycleState::Operational;
        assert!(finite_health(&hb, None).is_err());
        let latch = "4".repeat(64);
        hb.acknowledged_latch_sha256 = Some(latch.clone());
        assert!(finite_health(&hb, Some(&latch)).is_ok());
        hb.reason_code = Some(wire::HeartbeatReasonCode::Startup);
        assert!(finite_health(&hb, Some(&latch)).is_err());
        assert!(!loop_positive(
            wire::LoopState::Stopped,
            wire::ControlProgress::Stale,
            wire::LifecycleState::Parked
        ));
        use wire::{HeartbeatHealthState as H, HeartbeatReasonCode as R, LifecycleState as L};
        let reasons = [
            None,
            Some(R::ArtifactMismatch),
            Some(R::ConfigMismatch),
            Some(R::DependencyUnavailable),
            Some(R::Draining),
            Some(R::HeadMismatch),
            Some(R::MigrationMismatch),
            Some(R::ProbeFailed),
            Some(R::ProtocolMismatch),
            Some(R::Startup),
        ];
        for phase in [L::Parked, L::LatchAcknowledged, L::Operational] {
            for health in [H::Ready, H::Degraded, H::Draining] {
                for reason in reasons {
                    for ack in [None, Some(latch.clone()), Some("5".repeat(64))] {
                        for current in [None, Some(latch.as_str())] {
                            hb.lifecycle_state = phase;
                            hb.health_state = health;
                            hb.reason_code = reason;
                            hb.acknowledged_latch_sha256 = ack.clone();
                            let pair = (health == H::Ready && reason.is_none())
                                || (health == H::Draining && reason == Some(R::Draining))
                                || (health == H::Degraded
                                    && reason.is_some()
                                    && reason != Some(R::Draining));
                            let bound = current.is_some() && ack.as_deref() == current;
                            let permitted = pair
                                && match phase {
                                    L::Parked => ack.is_none() && health != H::Ready,
                                    L::LatchAcknowledged => bound && health != H::Ready,
                                    L::Operational => bound,
                                };
                            assert_eq!(finite_health(&hb, current).is_ok(), permitted);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn provisioning_runtime_request_peers_are_closed_and_debug_redacts_secrets() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = sqlite();
        let (base, binding) = fixture(&pool);
        let (request, claimed) = claim(
            &pool,
            &role_input(&base, "original_source_verifier"),
            &binding,
        );
        let registration_request = registration_request(&request, &claimed);
        let registration = register(&pool, &request, &claimed);
        let start = startup(&request, &claimed, &registration);
        let value = serde_json::to_value(&request).unwrap();
        for key in value.as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(wire::parse_claim_request(&json(&missing).unwrap()).is_err());
        }
        let mut extra = value;
        extra
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(wire::parse_claim_request(&json(&extra).unwrap()).is_err());
        let registration_value = serde_json::to_value(&registration_request).unwrap();
        for key in registration_value.as_object().unwrap().keys() {
            let mut missing = registration_value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(wire::parse_boot_registration_request(&json(&missing).unwrap()).is_err());
        }
        let startup_value = serde_json::to_value(&start).unwrap();
        for key in startup_value.as_object().unwrap().keys() {
            let mut missing = startup_value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(wire::parse_boot_startup_request(&json(&missing).unwrap()).is_err());
        }
        let duplicate = String::from_utf8(json(&registration_request).unwrap())
            .unwrap()
            .replacen("{", "{\"version\":2,", 1);
        assert!(wire::parse_boot_registration_request(duplicate.as_bytes()).is_err());
        let mut null_boot = startup_value;
        null_boot["bootId"] = serde_json::Value::Null;
        assert!(wire::parse_boot_startup_request(&json(&null_boot).unwrap()).is_err());
        assert!(!format!("{request:?} {registration_request:?} {start:?}")
            .contains(&request.session_token));
        assert!(!format!("{request:?}").contains(&request.grant_token));
    }

    #[test]
    fn provisioning_runtime_postgres_control_replay_and_parked_boot_when_configured() {
        let Some(url) = crate::db::owned_postgres_test::owned_url(
            "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_control_replay_and_parked_boot_when_configured",
        ) else {
            return;
        };
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        assert!(
            pool.get_pg()
                .unwrap()
                .query_one(
                    "SELECT to_regclass('public.jobs_managed_cloud_activations') IS NULL",
                    &[]
                )
                .unwrap()
                .get::<_, bool>(0),
            "requires a separately owned fresh nonproduction database"
        );
        crate::db::run_migrations(&pool).unwrap();
        let (base, binding) = fixture(&pool);
        replay_scenario(&pool, &base, &binding);
        osv_scenario(&pool, &base, &binding);
        concurrent_replay_scenario(&pool, &base, &binding);
        assert_eq!(count(&pool, "jobs_managed_cloud_runtime_instances"), 2);
        assert_eq!(
            count(
                &pool,
                "jobs_managed_cloud_original_source_verifier_runtime_instances"
            ),
            1
        );
        println!("phase655_owned_runtime_control_replay_complete");
    }

    #[test]
    fn provisioning_runtime_postgres_storage_only_v2_ready_lowering_when_configured() {
        let Some(url) = crate::db::owned_postgres_test::owned_url(
            "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_storage_only_v2_ready_lowering_when_configured",
        ) else {
            return;
        };
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        assert!(
            pool.get_pg()
                .unwrap()
                .query_one(
                    "SELECT to_regclass('public.jobs_managed_cloud_activations') IS NULL",
                    &[]
                )
                .unwrap()
                .get::<_, bool>(0),
            "requires a separately owned fresh nonproduction database"
        );
        crate::db::run_migrations(&pool).unwrap();
        let (base, binding) = fixture(&pool);
        storage_only_osv_lowering_scenario(&pool, &base, &binding);
        println!("phase655_owned_runtime_v2_ready_lowering_complete");
    }

    #[test]
    fn provisioning_runtime_postgres_legacy_boot_partition_when_configured() {
        let Some(url) = crate::db::owned_postgres_test::owned_url(
            "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_legacy_boot_partition_when_configured",
        ) else {
            return;
        };
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        assert!(
            pool.get_pg()
                .unwrap()
                .query_one(
                    "SELECT to_regclass('public.jobs_managed_cloud_activations') IS NULL",
                    &[]
                )
                .unwrap()
                .get::<_, bool>(0),
            "requires a separately owned fresh nonproduction database"
        );
        crate::db::run_migrations(&pool).unwrap();
        legacy_osv_partition_scenario(&pool);
        println!("phase655_owned_runtime_legacy_boot_partition_complete");
    }
}
