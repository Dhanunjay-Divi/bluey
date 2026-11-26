//! Candidate-only provisioning-v4 unsigned-draft adapter.
//!
//! This private `db::jobs` child has no SQL writer and returns neither raw
//! authority/configuration/trust material nor a final signature.  The reader
//! typed reader is composed below; runtime database qualification remains separate,
//! and this unsigned draft grants no serving authority.

use super::managed_cloud_provisioning_persistence::ProvisioningDocument;
use super::managed_cloud_provisioning_protocol::{AuthorityBinding, AuthorityKind, Purpose};
use super::managed_cloud_provisioning_runtime::ProvisioningProcessContext;
use super::*;
use base64::Engine;
use serde::{Deserialize, Serialize};

#[cfg(test)]
include!("managed_cloud_provisioning_snapshot_durable_helper.rs");

const SNAPSHOT_VERSION: i64 = 1;
const DATABASE_SNAPSHOT: &str = "database_snapshot";
const ADMIN_INTENT: &str = "admin_intent";
const VALIDATED_IMPORT_PROJECTION: &str = "validated_import_projection";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProvisioningSigningRequest {
    pub version: i64,
    pub scope: ManagedCloudScope,
    pub queue_generation_id: String,
    pub issuance_ref: String,
    pub manifest_sha256: String,
    pub cohort_sha256: String,
    pub activation_id: String,
    pub activation_generation: i64,
    pub issued_at_ms: i64,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
    pub maximum_inflight: i64,
    pub maximum_daily_admissions: i64,
    pub heartbeat_ttl_ms: i64,
    pub intended_storage_config_sha256: String,
    pub signature_set_id: String,
    pub requested_signed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProvisioningSigningResponse {
    pub version: i64,
    pub snapshot_at_ms: i64,
    pub document_canonical_base64url: String,
    pub document_sha256: String,
    pub unsigned_signature_set: UnsignedSignatureSet,
    pub signature_payload_base64url: String,
    pub sources: SnapshotSources,
    pub expected_head: ExpectedHead,
    pub queue: SnapshotQueue,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnsignedSignatureSet {
    pub version: i64,
    pub audience: String,
    pub signature_set_id: String,
    pub trust_generation: i64,
    pub role: String,
    pub target_audience: String,
    pub target_sha256: String,
    pub signed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotSources {
    pub storage_config_sha256: IntentDigestSource,
    pub failure_converter_sha256: IntentDigestSource,
    pub manifest: String,
    pub cohort: String,
    pub trust: String,
    pub queue: String,
    pub head: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IntentDigestSource {
    pub value: String,
    pub origin: String,
}

#[derive(Debug, Clone)]
struct ValidatedImportFailureConverterProjection {
    // The reader may construct this only from the exact manifest row after its
    // canonical release, signature/trust, revocation, role, queue, and head
    // validations. It is not a fresh artifact measurement.
    manifest_sha256: String,
    value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExpectedHead {
    pub revision: i64,
    pub transition_sha256: Option<String>,
    pub activation_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotQueue {
    pub queue_generation_id: String,
    pub issuance_ref: String,
    pub task_queue_sha256: String,
}

/// The caller receives typed request/context values only.  It never receives a
/// transaction, SQL string, raw canonical source object, signing key, or final
/// signature.
pub(crate) fn snapshot_provisioning_signing_request(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
    recorded_by: &str,
) -> ManagedCloudResult<ProvisioningSigningResponse> {
    validate_request(request, recorded_by)?;
    let facts = snapshot_facts_read_only(pool, process, request)?;
    build_response(request, facts)
}

fn validate_request(
    request: &ProvisioningSigningRequest,
    recorded_by: &str,
) -> ManagedCloudResult<()> {
    if request.version != SNAPSHOT_VERSION
        || !managed_cloud_actor(recorded_by)
        || !managed_cloud_route_id(&request.queue_generation_id)
        || !managed_cloud_route_id(&request.issuance_ref)
        || !managed_cloud_token(&request.activation_id, 128)
        || !managed_cloud_token(&request.signature_set_id, 128)
        || ![
            &request.manifest_sha256,
            &request.cohort_sha256,
            &request.intended_storage_config_sha256,
        ]
        .iter()
        .all(|value| managed_cloud_hex64(value))
        || ![
            request.activation_generation,
            request.issued_at_ms,
            request.not_before_ms,
            request.expires_at_ms,
            request.requested_signed_at_ms,
        ]
        .iter()
        .all(|value| managed_cloud_safe_integer(*value, false))
        || !managed_cloud_safe_integer(request.activation_generation, true)
        || request.not_before_ms < request.issued_at_ms
        || request.expires_at_ms <= request.not_before_ms
        || !(5_000..=300_000).contains(&request.heartbeat_ttl_ms)
        || !(0..=100_000).contains(&request.maximum_inflight)
        || !(0..=1_000_000).contains(&request.maximum_daily_admissions)
    {
        return Err(ManagedCloudRegistryError::InvalidRequest);
    }
    validate_managed_cloud_scope(&request.scope)?;
    validate_channel_limits(
        &request.scope,
        request.maximum_inflight,
        request.maximum_daily_admissions,
    )
}

// This mirrors the managed-cloud activation schema constraint in
// 033_jobs_managed_cloud_release_authority.sql: shadow is non-serving,
// while canary and general both require positive admission limits.
fn validate_channel_limits(
    scope: &ManagedCloudScope,
    maximum_inflight: i64,
    maximum_daily_admissions: i64,
) -> ManagedCloudResult<()> {
    let valid = match scope.channel.as_str() {
        "shadow" => maximum_inflight == 0 && maximum_daily_admissions == 0,
        "canary" | "general" => maximum_inflight > 0 && maximum_daily_admissions > 0,
        _ => false,
    };
    valid
        .then_some(())
        .ok_or(ManagedCloudRegistryError::InvalidRequest)
}

/// Database-derived facts only.  In particular, storage configuration is not
/// present: it is a closed authenticated admin draft intent in the request.
struct SnapshotFacts {
    snapshot_at_ms: i64,
    head_revision: i64,
    previous_transition_sha256: Option<String>,
    previous_activation_sha256: Option<String>,
    trust_generation: i64,
    channel_sequence: i64,
    manifest_sha256: String,
    cohort_sha256: String,
    manifest_signature_set_sha256: String,
    cohort_signature_set_sha256: String,
    feature_authority: ManagedCloudFeatureAuthority,
    temporal_namespace_sha256: String,
    task_queue_sha256: String,
    failure_converter_projection: ValidatedImportFailureConverterProjection,
    authority_binding: AuthorityBinding,
    signing_role: String,
    policy_valid_from_ms: i64,
    policy_expires_at_ms: i64,
    queue_generation_id: String,
    issuance_ref: String,
}

// Insert in the v2 snapshot child.  SnapshotFacts is the 853a… shape with
// storage_config_sha256 removed.  The request's intended storage hash is never
// inspected here.

fn snapshot_facts_read_only(
    pool: &DbPool,
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
) -> ManagedCloudResult<SnapshotFacts> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(managed_cloud_storage)?;
            let tx = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(managed_cloud_storage)?;
            let now = managed_cloud_db_now_sqlite(&tx)?;
            let facts = sqlite_snapshot_facts_v3(&tx, process, request, now)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(facts)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(managed_cloud_storage)?;
            let mut tx = connection.transaction().map_err(managed_cloud_storage)?;
            lock_operational_hold_shared_postgres_tx(&mut tx)
                .map_err(map_managed_cloud_operational_error)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            let now = managed_cloud_db_now_postgres(&mut tx)?;
            let facts = postgres_snapshot_facts_v3(&mut tx, process, request, now)?;
            tx.commit().map_err(managed_cloud_storage)?;
            Ok(facts)
        }
    })
}

type SnapshotHeadRow = (i64, String, String, i64, i64);
type SnapshotManifestProjectionValues = (
    String,
    i64,
    String,
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotProtocolProjectionRow {
    protocol_id: String,
    protocol_version: i64,
    schema_sha256: String,
    ordinal: i64,
}

#[derive(Clone)]
struct SnapshotProtocolRows {
    generic: Vec<SnapshotProtocolProjectionRow>,
    source_verification: Option<SnapshotProtocolProjectionRow>,
    startup: Option<SnapshotProtocolProjectionRow>,
    boot: Option<SnapshotProtocolProjectionRow>,
}

#[derive(Clone, Copy)]
struct SnapshotPolicyWindow {
    valid_from_ms: i64,
    expires_at_ms: i64,
}
struct SnapshotHeadFacts {
    revision: i64,
    transition: Option<String>,
    activation: Option<String>,
    sequence: i64,
}
struct SnapshotSignedMaterial<'a> {
    manifest_bytes: &'a [u8],
    manifest: &'a ManagedCloudReleaseAuthority,
    cohort_bytes: &'a [u8],
    cohort: &'a ManagedCloudCohortAuthority,
}

fn sqlite_snapshot_head_v3(
    tx: &rusqlite::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<Option<SnapshotHeadRow>> {
    tx.query_row(
        "SELECT head_revision,current_transition_sha256,current_activation_sha256,current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment=?1 AND region=?2 AND channel=?3",
        rusqlite::params![scope.environment, scope.region, scope.channel],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    ).optional().map_err(managed_cloud_storage)
}

fn postgres_snapshot_head_v3(
    tx: &mut postgres::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<Option<SnapshotHeadRow>> {
    tx.query_opt(
        "SELECT head_revision,current_transition_sha256,current_activation_sha256,current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment=$1 AND region=$2 AND channel=$3 FOR SHARE",
        &[&scope.environment, &scope.region, &scope.channel],
    ).map_err(managed_cloud_storage).map(|row| row.map(|r| (r.get(0),r.get(1),r.get(2),r.get(3),r.get(4))))
}

fn snapshot_head_facts_v3(
    head: Option<SnapshotHeadRow>,
    trust_generation: i64,
) -> ManagedCloudResult<SnapshotHeadFacts> {
    match head {
        None => Ok(SnapshotHeadFacts {
            revision: 0,
            transition: None,
            activation: None,
            sequence: 1,
        }),
        Some((revision, transition, activation, prior_trust, prior_sequence)) => {
            if revision <= 0
                || revision >= MANAGED_CLOUD_MAX_SAFE_INTEGER
                || !managed_cloud_hex64(&transition)
                || !managed_cloud_hex64(&activation)
                || !managed_cloud_safe_integer(prior_trust, true)
                || !managed_cloud_safe_integer(prior_sequence, true)
                || trust_generation < prior_trust
                || prior_sequence >= MANAGED_CLOUD_MAX_SAFE_INTEGER
            {
                return Err(ManagedCloudRegistryError::CompareAndSwapConflict);
            }
            Ok(SnapshotHeadFacts {
                revision,
                transition: Some(transition),
                activation: Some(activation),
                sequence: prior_sequence + 1,
            })
        }
    }
}

#[derive(Debug)]
struct SnapshotRows {
    manifest_trust_generation: i64,
    manifest_signature_set_sha256: String,
    feature_authority_sha256: String,
    feature_authority: ManagedCloudFeatureAuthority,
    failure_converter_sha256: String,
    manifest_canonical_base64url: String,
    manifest_published_at_ms: i64,
    cohort_id: String,
    cohort_generation: i64,
    cohort_rollout_mode: String,
    cohort_member_count: i64,
    cohort_approval_ref: String,
    cohort_signature_set_sha256: String,
    cohort_canonical_base64url: String,
    cohort_issued_at_ms: i64,
    cohort_not_before_ms: i64,
    cohort_expires_at_ms: i64,
    queue_generation_id: String,
    issuance_ref: String,
    namespace: String,
    task_queue: String,
    task_queue_sha256: String,
}

// These are constants, rather than assembled from any request field.  The
// reader has no generic query or transaction escape hatch.
const SQLITE_SNAPSHOT_ROWS: &str = "SELECT
 m.trust_generation,m.authorization_signature_set_sha256,m.feature_authority_sha256,
 m.cloud_distribution_enabled,m.workflow_command_dispatch_enabled,m.workflow_cleanup_enabled,
 m.direct_discovery_enabled,m.global_discovery_enabled,m.source_verification_enabled,
 m.failure_converter_sha256,m.canonical_manifest_base64url,m.published_at_ms,
 c.cohort_id,c.cohort_generation,c.rollout_mode,c.member_count,c.approval_ref,
 c.authorization_signature_set_sha256,c.canonical_cohort_base64url,c.issued_at_ms,
 c.not_before_ms,c.expires_at_ms,q.queue_generation_id,q.issuance_ref,q.namespace,
 q.task_queue,q.task_queue_sha256
 FROM jobs_managed_cloud_manifests m
 JOIN jobs_managed_cloud_cohorts c ON c.cohort_sha256=?2
  AND c.trust_generation=m.trust_generation AND c.environment=?3 AND c.region=?4 AND c.channel=?5
 JOIN jobs_managed_cloud_queue_generations q ON q.queue_generation_id=?6
  AND q.environment=?3 AND q.region=?4 AND q.channel=?5 AND q.purpose='production'
 WHERE m.manifest_sha256=?1";

const POSTGRES_SNAPSHOT_ROWS: &str = "SELECT
 m.trust_generation,m.authorization_signature_set_sha256,m.feature_authority_sha256,
 m.cloud_distribution_enabled,m.workflow_command_dispatch_enabled,m.workflow_cleanup_enabled,
 m.direct_discovery_enabled,m.global_discovery_enabled,m.source_verification_enabled,
 m.failure_converter_sha256,m.canonical_manifest_base64url,m.published_at_ms,
 c.cohort_id,c.cohort_generation,c.rollout_mode,c.member_count,c.approval_ref,
 c.authorization_signature_set_sha256,c.canonical_cohort_base64url,c.issued_at_ms,
 c.not_before_ms,c.expires_at_ms,q.queue_generation_id,q.issuance_ref,q.namespace,
 q.task_queue,q.task_queue_sha256
 FROM jobs_managed_cloud_manifests m
 JOIN jobs_managed_cloud_cohorts c ON c.cohort_sha256=$2
  AND c.trust_generation=m.trust_generation AND c.environment=$3 AND c.region=$4 AND c.channel=$5
 JOIN jobs_managed_cloud_queue_generations q ON q.queue_generation_id=$6
  AND q.environment=$3 AND q.region=$4 AND q.channel=$5 AND q.purpose='production'
 WHERE m.manifest_sha256=$1 FOR SHARE OF m,c,q";

fn sqlite_snapshot_rows_v3(
    tx: &rusqlite::Transaction<'_>,
    request: &ProvisioningSigningRequest,
) -> ManagedCloudResult<SnapshotRows> {
    tx.query_row(
        SQLITE_SNAPSHOT_ROWS,
        rusqlite::params![
            request.manifest_sha256,
            request.cohort_sha256,
            request.scope.environment,
            request.scope.region,
            request.scope.channel,
            request.queue_generation_id,
        ],
        |r| {
            Ok(SnapshotRows {
                manifest_trust_generation: r.get(0)?,
                manifest_signature_set_sha256: r.get(1)?,
                feature_authority_sha256: r.get(2)?,
                feature_authority: ManagedCloudFeatureAuthority {
                    cloud_distribution: r.get(3)?,
                    workflow_command_dispatch: r.get(4)?,
                    workflow_cleanup: r.get(5)?,
                    direct_discovery: r.get(6)?,
                    global_discovery: r.get(7)?,
                    source_verification: r.get(8)?,
                },
                failure_converter_sha256: r.get(9)?,
                manifest_canonical_base64url: r.get(10)?,
                manifest_published_at_ms: r.get(11)?,
                cohort_id: r.get(12)?,
                cohort_generation: r.get(13)?,
                cohort_rollout_mode: r.get(14)?,
                cohort_member_count: r.get(15)?,
                cohort_approval_ref: r.get(16)?,
                cohort_signature_set_sha256: r.get(17)?,
                cohort_canonical_base64url: r.get(18)?,
                cohort_issued_at_ms: r.get(19)?,
                cohort_not_before_ms: r.get(20)?,
                cohort_expires_at_ms: r.get(21)?,
                queue_generation_id: r.get(22)?,
                issuance_ref: r.get(23)?,
                namespace: r.get(24)?,
                task_queue: r.get(25)?,
                task_queue_sha256: r.get(26)?,
            })
        },
    )
    .optional()
    .map_err(managed_cloud_storage)?
    .ok_or(ManagedCloudRegistryError::NotFound)
}

fn postgres_snapshot_rows_v3(
    tx: &mut postgres::Transaction<'_>,
    request: &ProvisioningSigningRequest,
) -> ManagedCloudResult<SnapshotRows> {
    tx.query_opt(
        POSTGRES_SNAPSHOT_ROWS,
        &[
            &request.manifest_sha256,
            &request.cohort_sha256,
            &request.scope.environment,
            &request.scope.region,
            &request.scope.channel,
            &request.queue_generation_id,
        ],
    )
    .map_err(managed_cloud_storage)?
    .map(|r| SnapshotRows {
        manifest_trust_generation: r.get(0),
        manifest_signature_set_sha256: r.get(1),
        feature_authority_sha256: r.get(2),
        feature_authority: ManagedCloudFeatureAuthority {
            cloud_distribution: r.get(3),
            workflow_command_dispatch: r.get(4),
            workflow_cleanup: r.get(5),
            direct_discovery: r.get(6),
            global_discovery: r.get(7),
            source_verification: r.get(8),
        },
        failure_converter_sha256: r.get(9),
        manifest_canonical_base64url: r.get(10),
        manifest_published_at_ms: r.get(11),
        cohort_id: r.get(12),
        cohort_generation: r.get(13),
        cohort_rollout_mode: r.get(14),
        cohort_member_count: r.get(15),
        cohort_approval_ref: r.get(16),
        cohort_signature_set_sha256: r.get(17),
        cohort_canonical_base64url: r.get(18),
        cohort_issued_at_ms: r.get(19),
        cohort_not_before_ms: r.get(20),
        cohort_expires_at_ms: r.get(21),
        queue_generation_id: r.get(22),
        issuance_ref: r.get(23),
        namespace: r.get(24),
        task_queue: r.get(25),
        task_queue_sha256: r.get(26),
    })
    .ok_or(ManagedCloudRegistryError::NotFound)
}

fn checked_snapshot_material_v3(
    rows: &SnapshotRows,
    request: &ProvisioningSigningRequest,
) -> ManagedCloudResult<(
    Vec<u8>,
    ManagedCloudReleaseAuthority,
    Vec<u8>,
    ManagedCloudCohortAuthority,
)> {
    if rows.queue_generation_id != request.queue_generation_id
        || rows.issuance_ref != request.issuance_ref
        || managed_cloud_task_queue_sha256(&rows.namespace, &rows.task_queue)?
            != rows.task_queue_sha256
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    let manifest_bytes = managed_cloud_decode_base64url(&rows.manifest_canonical_base64url)?;
    let manifest: ManagedCloudReleaseAuthority = managed_cloud_parse_canonical(&manifest_bytes)?;
    let cohort_bytes = managed_cloud_decode_base64url(&rows.cohort_canonical_base64url)?;
    let cohort: ManagedCloudCohortAuthority = managed_cloud_parse_canonical(&cohort_bytes)?;
    validate_managed_cloud_release(&manifest)?;
    validate_managed_cloud_cohort(&cohort)?;
    if managed_cloud_sha256(&manifest_bytes) != request.manifest_sha256
        || managed_cloud_sha256(&cohort_bytes) != request.cohort_sha256
        || cohort.trust_generation != rows.manifest_trust_generation
        || cohort.scope != request.scope
        || cohort.cohort_id != rows.cohort_id
        || cohort.cohort_generation != rows.cohort_generation
        || cohort.rollout_mode != rows.cohort_rollout_mode
        || i64::try_from(cohort.account_id_sha256s.len())
            .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?
            != rows.cohort_member_count
        || cohort.approval_ref != rows.cohort_approval_ref
        || manifest.feature_authority_sha256 != rows.feature_authority_sha256
        || manifest.feature_authority != rows.feature_authority
        || managed_cloud_digest(&manifest.feature_authority)? != rows.feature_authority_sha256
        || !managed_cloud_hex64(&rows.failure_converter_sha256)
        || manifest.published_at_ms != rows.manifest_published_at_ms
        || cohort.issued_at_ms != rows.cohort_issued_at_ms
        || cohort.not_before_ms != rows.cohort_not_before_ms
        || cohort.expires_at_ms != rows.cohort_expires_at_ms
        || managed_cloud_required_roles_for_feature(&manifest.feature_authority).len() != 9
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok((manifest_bytes, manifest, cohort_bytes, cohort))
}

fn snapshot_authority_binding_v3(
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
) -> ManagedCloudResult<AuthorityBinding> {
    let binding = AuthorityBinding {
        authority_kind: AuthorityKind::ProvisioningV4,
        purpose: Purpose::Production,
        context_sha256: None,
        queue_generation_id: request.queue_generation_id.clone(),
    };
    process.permits(&binding)?;
    Ok(binding)
}

fn sqlite_snapshot_facts_v3(
    tx: &rusqlite::Transaction<'_>,
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<SnapshotFacts> {
    let rows = sqlite_snapshot_rows_v3(tx, request)?;
    let (manifest_bytes, manifest, cohort_bytes, cohort) =
        checked_snapshot_material_v3(&rows, request)?;
    sqlite_snapshot_manifest_projection_v4(tx, request, &manifest)?;
    let policy = sqlite_managed_cloud_policy_by_generation(tx, rows.manifest_trust_generation)?;
    snapshot_validate_trust_sqlite_v3(
        tx,
        &policy,
        SnapshotSignedMaterial {
            manifest_bytes: &manifest_bytes,
            manifest: &manifest,
            cohort_bytes: &cohort_bytes,
            cohort: &cohort,
        },
        &rows,
        request,
        now,
    )?;
    snapshot_validate_roles_sqlite_v3(tx, &manifest, request, now)?;
    let head = snapshot_head_facts_v3(
        sqlite_snapshot_head_v3(tx, &request.scope)?,
        rows.manifest_trust_generation,
    )?;
    snapshot_facts_from_rows_v3(
        process,
        request,
        now,
        rows,
        SnapshotPolicyWindow {
            valid_from_ms: policy.policy.valid_from_ms,
            expires_at_ms: policy.policy.expires_at_ms,
        },
        head,
    )
}

fn postgres_snapshot_facts_v3(
    tx: &mut postgres::Transaction<'_>,
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<SnapshotFacts> {
    let rows = postgres_snapshot_rows_v3(tx, request)?;
    let (manifest_bytes, manifest, cohort_bytes, cohort) =
        checked_snapshot_material_v3(&rows, request)?;
    postgres_snapshot_manifest_projection_v4(tx, request, &manifest)?;
    let policy = postgres_managed_cloud_policy_by_generation(tx, rows.manifest_trust_generation)?;
    snapshot_validate_trust_postgres_v3(
        tx,
        &policy,
        SnapshotSignedMaterial {
            manifest_bytes: &manifest_bytes,
            manifest: &manifest,
            cohort_bytes: &cohort_bytes,
            cohort: &cohort,
        },
        &rows,
        request,
        now,
    )?;
    snapshot_validate_roles_postgres_v3(tx, &manifest, request, now)?;
    let head = snapshot_head_facts_v3(
        postgres_snapshot_head_v3(tx, &request.scope)?,
        rows.manifest_trust_generation,
    )?;
    snapshot_facts_from_rows_v3(
        process,
        request,
        now,
        rows,
        SnapshotPolicyWindow {
            valid_from_ms: policy.policy.valid_from_ms,
            expires_at_ms: policy.policy.expires_at_ms,
        },
        head,
    )
}

fn snapshot_facts_from_rows_v3(
    process: &ProvisioningProcessContext,
    request: &ProvisioningSigningRequest,
    now: i64,
    rows: SnapshotRows,
    policy_window: SnapshotPolicyWindow,
    head: SnapshotHeadFacts,
) -> ManagedCloudResult<SnapshotFacts> {
    let authority_binding = snapshot_authority_binding_v3(process, request)?;
    Ok(SnapshotFacts {
        snapshot_at_ms: now,
        head_revision: head.revision,
        previous_transition_sha256: head.transition,
        previous_activation_sha256: head.activation,
        trust_generation: rows.manifest_trust_generation,
        channel_sequence: head.sequence,
        manifest_sha256: request.manifest_sha256.clone(),
        cohort_sha256: request.cohort_sha256.clone(),
        manifest_signature_set_sha256: rows.manifest_signature_set_sha256,
        cohort_signature_set_sha256: rows.cohort_signature_set_sha256,
        feature_authority: rows.feature_authority,
        temporal_namespace_sha256: managed_cloud_sha256(rows.namespace.as_bytes()),
        task_queue_sha256: rows.task_queue_sha256,
        // This is the exact immutable manifest-row projection established by
        // authenticated import.  It is not a release field, freshly rederived
        // inventory fact, deployed-file readback, or readiness assertion.
        failure_converter_projection: validated_import_failure_converter_projection_v5(
            &request.manifest_sha256,
            &request.manifest_sha256,
            rows.failure_converter_sha256,
        )?,
        authority_binding,
        signing_role: if request.scope.channel == "general" {
            "general_promotion"
        } else {
            "promotion"
        }
        .into(),
        policy_valid_from_ms: policy_window.valid_from_ms,
        policy_expires_at_ms: policy_window.expires_at_ms,
        queue_generation_id: rows.queue_generation_id,
        issuance_ref: rows.issuance_ref,
    })
}

fn validated_import_failure_converter_projection_v5(
    requested_manifest_sha256: &str,
    row_manifest_sha256: &str,
    value: String,
) -> ManagedCloudResult<ValidatedImportFailureConverterProjection> {
    if requested_manifest_sha256 != row_manifest_sha256 || !managed_cloud_hex64(&value) {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(ValidatedImportFailureConverterProjection {
        manifest_sha256: row_manifest_sha256.to_owned(),
        value,
    })
}

#[cfg(test)]
mod snapshot_reader_v5_tests {
    use super::*;

    #[test]
    fn validated_import_projection_refuses_a_different_manifest_row() {
        let result = validated_import_failure_converter_projection_v5(
            &"a".repeat(64),
            &"b".repeat(64),
            "c".repeat(64),
        );
        assert!(matches!(
            result,
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
    }

    #[test]
    fn validated_import_projection_refuses_noncanonical_digest() {
        let result = validated_import_failure_converter_projection_v5(
            &"a".repeat(64),
            &"a".repeat(64),
            "A".repeat(64),
        );
        assert!(matches!(
            result,
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
    }
}

#[cfg(test)]
mod projection_tests {
    use super::super::managed_cloud_release_authority_tests as legacy_tests;
    use super::*;

    fn release(version: i64) -> ManagedCloudReleaseAuthority {
        if version != 1 {
            return legacy_tests::managed_cloud_test_release_import(1_000, version).0;
        }
        let mut legacy = legacy_tests::managed_cloud_test_release_import(1_000, 2).0;
        legacy.version = 1;
        legacy.audience = MANAGED_CLOUD_RELEASE_AUDIENCE.into();
        legacy.feature_authority.direct_discovery = false;
        legacy.feature_authority.global_discovery = false;
        legacy.feature_authority.source_verification = false;
        legacy
            .protocols
            .retain(|protocol| protocol.protocol_id != "source_verification");
        let contract = managed_cloud_release_contract(1, MANAGED_CLOUD_RELEASE_AUDIENCE)
            .expect("v1 release contract");
        assert_eq!(legacy.protocols.len(), contract.protocols.len());
        assert!(legacy.protocols.iter().zip(contract.protocols).all(
            |(protocol, (id, version))| {
                protocol.protocol_id == *id && protocol.protocol_version == *version
            }
        ));
        legacy
    }

    fn special_row_mut(
        rows: &mut SnapshotProtocolRows,
        index: usize,
    ) -> &mut Option<SnapshotProtocolProjectionRow> {
        match index {
            0 => &mut rows.source_verification,
            1 => &mut rows.startup,
            2 => &mut rows.boot,
            _ => panic!("test-only specialized protocol index"),
        }
    }

    fn row_for(
        release: &ManagedCloudReleaseAuthority,
        protocol_id: &str,
    ) -> SnapshotProtocolProjectionRow {
        let (ordinal, protocol) = release
            .protocols
            .iter()
            .enumerate()
            .find(|(_, protocol)| protocol.protocol_id == protocol_id)
            .expect("canonical protocol exists");
        SnapshotProtocolProjectionRow {
            protocol_id: protocol.protocol_id.clone(),
            protocol_version: protocol.protocol_version,
            schema_sha256: protocol.schema_sha256.clone(),
            ordinal: i64::try_from(ordinal).expect("test ordinal fits"),
        }
    }

    fn rows_for(release: &ManagedCloudReleaseAuthority) -> SnapshotProtocolRows {
        let is_special = |protocol_id: &str| {
            matches!(
                protocol_id,
                "source_verification" | "source_verifier_startup" | "source_verifier_boot"
            )
        };
        SnapshotProtocolRows {
            generic: release
                .protocols
                .iter()
                .enumerate()
                .filter(|(_, protocol)| !is_special(&protocol.protocol_id))
                .map(|(ordinal, protocol)| SnapshotProtocolProjectionRow {
                    protocol_id: protocol.protocol_id.clone(),
                    protocol_version: protocol.protocol_version,
                    schema_sha256: protocol.schema_sha256.clone(),
                    ordinal: i64::try_from(ordinal).expect("test ordinal fits"),
                })
                .collect(),
            source_verification: release
                .feature_authority
                .source_verification
                .then(|| row_for(release, "source_verification")),
            startup: matches!(release.version, 3 | 4)
                .then(|| row_for(release, "source_verifier_startup")),
            boot: matches!(release.version, 3 | 4)
                .then(|| row_for(release, "source_verifier_boot")),
        }
    }

    fn assert_identity_conflict(result: ManagedCloudResult<()>) {
        assert!(matches!(
            result,
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
    }

    #[test]
    fn protocol_projection_v3_uses_eleven_generic_rows_not_fourteen() {
        let release = release(3);
        let rows = rows_for(&release);
        assert_eq!(release.protocols.len(), 14);
        assert_eq!(rows.generic.len(), 11);
        assert!(snapshot_protocol_projection_values_v4(11, rows.clone(), &release).is_ok());
        assert_identity_conflict(snapshot_protocol_projection_values_v4(14, rows, &release));
    }

    #[test]
    fn protocol_projection_v4_accepts_exact_specialized_rows_and_refuses_missing() {
        let release = release(4);
        let rows = rows_for(&release);
        assert_eq!(release.protocols.len(), 14);
        assert_eq!(rows.generic.len(), 11);
        assert!(snapshot_protocol_projection_values_v4(11, rows.clone(), &release).is_ok());
        for index in 0..3 {
            let mut missing = rows.clone();
            *special_row_mut(&mut missing, index) = None;
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11, missing, &release,
            ));
        }
    }

    #[test]
    fn protocol_projection_accepts_v1_and_v2_layouts() {
        let v1 = release(1);
        assert!(snapshot_protocol_projection_values_v4(11, rows_for(&v1), &v1).is_ok());

        let v2 = release(2);
        let v2_rows = rows_for(&v2);
        assert_eq!(v2.protocols.len(), 12);
        assert_eq!(v2_rows.generic.len(), 11);
        assert!(snapshot_protocol_projection_values_v4(11, v2_rows.clone(), &v2).is_ok());

        let mut missing_source = v2_rows.clone();
        missing_source.source_verification = None;
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            missing_source,
            &v2,
        ));
        let mut changed_source = v2_rows.clone();
        changed_source
            .source_verification
            .as_mut()
            .expect("v2 source row")
            .schema_sha256 = "f".repeat(64);
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            changed_source,
            &v2,
        ));

        let v3 = release(3);
        let mut unexpected_startup = v2_rows.clone();
        unexpected_startup.startup = Some(row_for(&v3, "source_verifier_startup"));
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            unexpected_startup,
            &v2,
        ));
        let mut unexpected_boot = v2_rows;
        unexpected_boot.boot = Some(row_for(&v3, "source_verifier_boot"));
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            unexpected_boot,
            &v2,
        ));
    }

    #[test]
    fn protocol_projection_refuses_missing_or_changed_specialized_rows() {
        let release = release(3);
        let rows = rows_for(&release);
        assert!(snapshot_protocol_projection_values_v4(11, rows.clone(), &release).is_ok());
        for index in 0..3 {
            let mut missing = rows.clone();
            *special_row_mut(&mut missing, index) = None;
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11, missing, &release,
            ));

            let mut changed_id = rows.clone();
            special_row_mut(&mut changed_id, index)
                .as_mut()
                .expect("specialized row")
                .protocol_id = "changed_protocol".into();
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11, changed_id, &release,
            ));

            let mut changed_version = rows.clone();
            special_row_mut(&mut changed_version, index)
                .as_mut()
                .expect("specialized row")
                .protocol_version += 1;
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11,
                changed_version,
                &release,
            ));

            let mut changed_hash = rows.clone();
            special_row_mut(&mut changed_hash, index)
                .as_mut()
                .expect("specialized row")
                .schema_sha256 = "f".repeat(64);
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11,
                changed_hash,
                &release,
            ));

            let mut changed_ordinal = rows.clone();
            special_row_mut(&mut changed_ordinal, index)
                .as_mut()
                .expect("specialized row")
                .ordinal = 99;
            assert_identity_conflict(snapshot_protocol_projection_values_v4(
                11,
                changed_ordinal,
                &release,
            ));
        }
    }

    #[test]
    fn protocol_projection_refuses_legacy_specialized_rows() {
        let legacy_release = release(1);
        let mut rows = rows_for(&legacy_release);
        rows.source_verification = Some(row_for(&release(2), "source_verification"));
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            rows,
            &legacy_release,
        ));
    }

    #[test]
    fn protocol_projection_refuses_changed_generic_rows_and_ordinals() {
        let release = release(3);
        let rows = rows_for(&release);
        assert!(snapshot_protocol_projection_values_v4(11, rows.clone(), &release).is_ok());
        let mut changed_protocol = rows.clone();
        changed_protocol.generic[0].protocol_version += 1;
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            changed_protocol,
            &release,
        ));
        let mut changed_hash = rows.clone();
        changed_hash.generic[0].schema_sha256 = "f".repeat(64);
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            changed_hash,
            &release,
        ));
        let mut removed = rows.clone();
        removed.generic.pop();
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11, removed, &release,
        ));
        let mut added = rows.clone();
        added.generic.push(added.generic[0].clone());
        assert_identity_conflict(snapshot_protocol_projection_values_v4(11, added, &release));
        let mut reordered = rows.clone();
        reordered.generic.swap(0, 1);
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11, reordered, &release,
        ));
        let mut changed_ordinal = rows;
        changed_ordinal
            .generic
            .last_mut()
            .expect("generic row")
            .ordinal = 99;
        assert_identity_conflict(snapshot_protocol_projection_values_v4(
            11,
            changed_ordinal,
            &release,
        ));
    }
}

fn sqlite_snapshot_signature_v3(
    tx: &rusqlite::Transaction<'_>,
    sha: &str,
    audience: &str,
    target: &str,
    generation: i64,
    role: &str,
) -> ManagedCloudResult<ManagedCloudSignatureSetAuthority> {
    let canonical: String = tx.query_row(
        "SELECT canonical_signature_set_base64url FROM jobs_managed_cloud_signature_sets WHERE signature_set_sha256=?1 AND target_audience=?2 AND target_sha256=?3 AND trust_generation=?4 AND role=?5",
        rusqlite::params![sha, audience, target, generation, role], |r| r.get(0),
    ).map_err(managed_cloud_storage)?;
    let bytes = managed_cloud_decode_base64url(&canonical)?;
    if managed_cloud_sha256(&bytes) != sha {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    managed_cloud_parse_canonical(&bytes)
}

fn postgres_snapshot_signature_v3(
    tx: &mut postgres::Transaction<'_>,
    sha: &str,
    audience: &str,
    target: &str,
    generation: i64,
    role: &str,
) -> ManagedCloudResult<ManagedCloudSignatureSetAuthority> {
    let row = tx.query_one(
        "SELECT canonical_signature_set_base64url FROM jobs_managed_cloud_signature_sets WHERE signature_set_sha256=$1 AND target_audience=$2 AND target_sha256=$3 AND trust_generation=$4 AND role=$5 FOR SHARE",
        &[&sha, &audience, &target, &generation, &role],
    ).map_err(managed_cloud_storage)?;
    let canonical: String = row.get(0);
    let bytes = managed_cloud_decode_base64url(&canonical)?;
    if managed_cloud_sha256(&bytes) != sha {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    managed_cloud_parse_canonical(&bytes)
}

fn snapshot_verify_target_v3(
    bytes: &[u8],
    signature: &ManagedCloudSignatureSetAuthority,
    policy: &StoredManagedCloudTrustPolicy,
    role: &str,
    audience: &str,
    issued: i64,
    now: i64,
) -> ManagedCloudResult<()> {
    validate_managed_cloud_trust_policy(&policy.policy)?;
    verify_managed_cloud_signature_set(
        bytes,
        signature,
        &policy.policy,
        role,
        audience,
        issued,
        now,
    )
}

fn snapshot_validate_trust_sqlite_v3(
    tx: &rusqlite::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    material: SnapshotSignedMaterial<'_>,
    rows: &SnapshotRows,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<()> {
    let SnapshotSignedMaterial {
        manifest_bytes,
        manifest,
        cohort_bytes,
        cohort,
    } = material;
    let promotion = if request.scope.channel == "general" {
        "general_promotion"
    } else {
        "promotion"
    };
    let manifest_sig = sqlite_snapshot_signature_v3(
        tx,
        &rows.manifest_signature_set_sha256,
        &manifest.audience,
        &request.manifest_sha256,
        rows.manifest_trust_generation,
        "release",
    )?;
    let cohort_sig = sqlite_snapshot_signature_v3(
        tx,
        &rows.cohort_signature_set_sha256,
        MANAGED_CLOUD_COHORT_AUDIENCE,
        &request.cohort_sha256,
        rows.manifest_trust_generation,
        promotion,
    )?;
    snapshot_verify_target_v3(
        manifest_bytes,
        &manifest_sig,
        policy,
        "release",
        &manifest.audience,
        manifest.published_at_ms,
        now,
    )?;
    snapshot_verify_target_v3(
        cohort_bytes,
        &cohort_sig,
        policy,
        promotion,
        MANAGED_CLOUD_COHORT_AUDIENCE,
        cohort.issued_at_ms,
        now,
    )?;
    require_sqlite_managed_cloud_signing_authority_not_revoked(tx, policy, &manifest_sig, now)?;
    require_sqlite_managed_cloud_signing_authority_not_revoked(tx, policy, &cohort_sig, now)?;
    if now < cohort.not_before_ms || now >= cohort.expires_at_ms || request.requested_signed_at_ms < policy.policy.valid_from_ms || request.requested_signed_at_ms >= policy.policy.expires_at_ms || tx.query_row("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=?1 AND ((subject_kind IN ('manifest','release') AND subject_sha256=?2) OR (subject_kind='cohort' AND subject_sha256=?3)) LIMIT 1", rusqlite::params![now, request.manifest_sha256, request.cohort_sha256], |_| Ok(())).optional().map_err(managed_cloud_storage)?.is_some() { return Err(ManagedCloudRegistryError::Revoked); }
    snapshot_validate_policy_chain_sqlite_v3(tx, policy, now)
}

fn snapshot_validate_trust_postgres_v3(
    tx: &mut postgres::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    material: SnapshotSignedMaterial<'_>,
    rows: &SnapshotRows,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<()> {
    let SnapshotSignedMaterial {
        manifest_bytes,
        manifest,
        cohort_bytes,
        cohort,
    } = material;
    let promotion = if request.scope.channel == "general" {
        "general_promotion"
    } else {
        "promotion"
    };
    let manifest_sig = postgres_snapshot_signature_v3(
        tx,
        &rows.manifest_signature_set_sha256,
        &manifest.audience,
        &request.manifest_sha256,
        rows.manifest_trust_generation,
        "release",
    )?;
    let cohort_sig = postgres_snapshot_signature_v3(
        tx,
        &rows.cohort_signature_set_sha256,
        MANAGED_CLOUD_COHORT_AUDIENCE,
        &request.cohort_sha256,
        rows.manifest_trust_generation,
        promotion,
    )?;
    snapshot_verify_target_v3(
        manifest_bytes,
        &manifest_sig,
        policy,
        "release",
        &manifest.audience,
        manifest.published_at_ms,
        now,
    )?;
    snapshot_verify_target_v3(
        cohort_bytes,
        &cohort_sig,
        policy,
        promotion,
        MANAGED_CLOUD_COHORT_AUDIENCE,
        cohort.issued_at_ms,
        now,
    )?;
    require_postgres_managed_cloud_signing_authority_not_revoked(tx, policy, &manifest_sig, now)?;
    require_postgres_managed_cloud_signing_authority_not_revoked(tx, policy, &cohort_sig, now)?;
    if now < cohort.not_before_ms || now >= cohort.expires_at_ms || request.requested_signed_at_ms < policy.policy.valid_from_ms || request.requested_signed_at_ms >= policy.policy.expires_at_ms || tx.query_opt("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1 AND ((subject_kind IN ('manifest','release') AND subject_sha256=$2) OR (subject_kind='cohort' AND subject_sha256=$3)) LIMIT 1", &[&now, &request.manifest_sha256, &request.cohort_sha256]).map_err(managed_cloud_storage)?.is_some() { return Err(ManagedCloudRegistryError::Revoked); }
    snapshot_validate_policy_chain_postgres_v3(tx, policy, now)
}

fn snapshot_validate_policy_chain_sqlite_v3(
    tx: &rusqlite::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    now: i64,
) -> ManagedCloudResult<()> {
    let bytes = managed_cloud_decode_base64url(&policy.canonical_policy_base64url)?;
    let signature = sqlite_snapshot_signature_v3(
        tx,
        &policy.authorization_signature_set_sha256,
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
        &policy.policy_sha256,
        policy.policy.trust_generation,
        "root",
    )?;
    if policy.policy.trust_generation == 1 {
        verify_managed_cloud_bootstrap_union_signature_set(
            &bytes,
            &signature,
            &managed_cloud_root_anchor_from_environment()?,
            &policy.policy,
            now,
        )?;
    } else {
        let previous =
            sqlite_managed_cloud_policy_by_generation(tx, policy.policy.trust_generation - 1)?;
        require_sqlite_managed_cloud_signing_authority_not_revoked(tx, &previous, &signature, now)?;
        verify_managed_cloud_rotation_union_signature_set(
            &bytes,
            &signature,
            &previous.policy,
            &policy.policy,
            now,
        )?;
    }
    require_sqlite_managed_cloud_signing_authority_not_revoked(tx, policy, &signature, now)
}

fn snapshot_validate_policy_chain_postgres_v3(
    tx: &mut postgres::Transaction<'_>,
    policy: &StoredManagedCloudTrustPolicy,
    now: i64,
) -> ManagedCloudResult<()> {
    let bytes = managed_cloud_decode_base64url(&policy.canonical_policy_base64url)?;
    let signature = postgres_snapshot_signature_v3(
        tx,
        &policy.authorization_signature_set_sha256,
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
        &policy.policy_sha256,
        policy.policy.trust_generation,
        "root",
    )?;
    if policy.policy.trust_generation == 1 {
        verify_managed_cloud_bootstrap_union_signature_set(
            &bytes,
            &signature,
            &managed_cloud_root_anchor_from_environment()?,
            &policy.policy,
            now,
        )?;
    } else {
        let previous =
            postgres_managed_cloud_policy_by_generation(tx, policy.policy.trust_generation - 1)?;
        require_postgres_managed_cloud_signing_authority_not_revoked(
            tx, &previous, &signature, now,
        )?;
        verify_managed_cloud_rotation_union_signature_set(
            &bytes,
            &signature,
            &previous.policy,
            &policy.policy,
            now,
        )?;
    }
    require_postgres_managed_cloud_signing_authority_not_revoked(tx, policy, &signature, now)
}

fn snapshot_validate_roles_sqlite_v3(
    tx: &rusqlite::Transaction<'_>,
    manifest: &ManagedCloudReleaseAuthority,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<()> {
    let roles = managed_cloud_required_roles_for_feature(&manifest.feature_authority);
    if roles.len() != 9 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    for role in roles {
        let pair: (String, String) = tx.query_row("SELECT capability.component_id,component.artifact_sha256 FROM jobs_managed_cloud_manifest_capabilities capability JOIN jobs_managed_cloud_manifest_components component ON component.manifest_sha256=capability.manifest_sha256 AND component.component_id=capability.component_id WHERE capability.manifest_sha256=?1 AND capability.capability=?2", rusqlite::params![request.manifest_sha256, role], |r| Ok((r.get(0)?, r.get(1)?))).map_err(managed_cloud_storage)?;
        if !manifest
            .components
            .iter()
            .any(|c| c.component_id == pair.0 && c.artifact_sha256 == pair.1)
            || !manifest
                .capabilities
                .iter()
                .any(|c| c.component_id == pair.0 && c.capability == role)
        {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        if tx.query_row("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=?1 AND subject_kind='component' AND subject_id=?2 AND subject_sha256=?3 LIMIT 1", rusqlite::params![now, pair.0, pair.1], |_| Ok(())).optional().map_err(managed_cloud_storage)?.is_some() {
            return Err(ManagedCloudRegistryError::Revoked);
        }
    }
    Ok(())
}

fn snapshot_validate_roles_postgres_v3(
    tx: &mut postgres::Transaction<'_>,
    manifest: &ManagedCloudReleaseAuthority,
    request: &ProvisioningSigningRequest,
    now: i64,
) -> ManagedCloudResult<()> {
    let roles = managed_cloud_required_roles_for_feature(&manifest.feature_authority);
    if roles.len() != 9 {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    for role in roles {
        let row = tx.query_one("SELECT capability.component_id,component.artifact_sha256 FROM jobs_managed_cloud_manifest_capabilities capability JOIN jobs_managed_cloud_manifest_components component ON component.manifest_sha256=capability.manifest_sha256 AND component.component_id=capability.component_id WHERE capability.manifest_sha256=$1 AND capability.capability=$2 FOR SHARE OF capability,component", &[&request.manifest_sha256, &role]).map_err(managed_cloud_storage)?;
        let pair: (String, String) = (row.get(0), row.get(1));
        if !manifest
            .components
            .iter()
            .any(|c| c.component_id == pair.0 && c.artifact_sha256 == pair.1)
            || !manifest
                .capabilities
                .iter()
                .any(|c| c.component_id == pair.0 && c.capability == role)
        {
            return Err(ManagedCloudRegistryError::IdentityConflict);
        }
        if tx.query_opt("SELECT 1 FROM jobs_managed_cloud_revocations WHERE effective_at_ms<=$1 AND subject_kind='component' AND subject_id=$2 AND subject_sha256=$3 LIMIT 1", &[&now, &pair.0, &pair.1]).map_err(managed_cloud_storage)?.is_some() {
            return Err(ManagedCloudRegistryError::Revoked);
        }
    }
    Ok(())
}

// The failure-converter digest is deliberately excluded from the canonical
// release comparison: it is not a release-authority field. It is a protected,
// validated-import manifest-row projection, produced at import from authenticated
// inventory/evidence and protected against later mutation. Source inventory and
// evidence bytes are not retained, so this reader does not freshly rederive it
// and makes no claim about an actual deployed file or runtime readiness.
const SNAPSHOT_MANIFEST_PROJECTION: &str = "SELECT manifest_id,manifest_generation,release_id,release_sequence,source_commit,sqlite_migration_head,postgres_migration_head,migration_set_sha256,config_schema_sha256,protocol_set_sha256,component_set_sha256,verification_evidence_sha256,component_count,capability_count,protocol_count FROM jobs_managed_cloud_manifests WHERE manifest_sha256=";
const SQLITE_SNAPSHOT_GENERIC_PROTOCOL_ROWS: &str = "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_protocols WHERE manifest_sha256=?1 ORDER BY ordinal";
const POSTGRES_SNAPSHOT_GENERIC_PROTOCOL_ROWS: &str = "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_protocols WHERE manifest_sha256=$1 ORDER BY ordinal FOR SHARE";
const SQLITE_SNAPSHOT_SOURCE_PROTOCOL_ROWS: [&str; 3] = [
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verification_protocols WHERE manifest_sha256=?1",
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols WHERE manifest_sha256=?1",
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols WHERE manifest_sha256=?1",
];
const POSTGRES_SNAPSHOT_SOURCE_PROTOCOL_ROWS: [&str; 3] = [
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verification_protocols WHERE manifest_sha256=$1 FOR SHARE",
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verifier_startup_protocols WHERE manifest_sha256=$1 FOR SHARE",
    "SELECT protocol_id,protocol_version,schema_sha256,ordinal FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols WHERE manifest_sha256=$1 FOR SHARE",
];

fn snapshot_manifest_projection_values_v4(
    values: SnapshotManifestProjectionValues,
    protocol_rows: SnapshotProtocolRows,
    manifest: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    let (
        manifest_id,
        manifest_generation,
        release_id,
        release_sequence,
        source_commit,
        sqlite_head,
        postgres_head,
        migration,
        config,
        protocol,
        component_set,
        evidence,
        components,
        capabilities,
        protocols,
    ) = values;
    if manifest_id != manifest.manifest_id
        || manifest_generation != manifest.manifest_generation
        || release_id != manifest.release_id
        || release_sequence != manifest.release_sequence
        || source_commit != manifest.source_commit
        || sqlite_head != manifest.sqlite_migration_head
        || postgres_head != manifest.postgres_migration_head
        || migration != manifest.migration_set_sha256
        || config != manifest.config_schema_sha256
        || protocol != manifest.protocol_set_sha256
        || component_set != manifest.component_set_sha256
        || evidence != manifest.verification_evidence_sha256
        || components
            != i64::try_from(manifest.components.len())
                .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?
        || capabilities
            != i64::try_from(manifest.capabilities.len())
                .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    snapshot_protocol_projection_values_v4(protocols, protocol_rows, manifest)
}

fn snapshot_protocol_projection_values_v4(
    stored_generic_count: i64,
    protocol_rows: SnapshotProtocolRows,
    manifest: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    const SPECIAL_PROTOCOLS: [&str; 3] = [
        "source_verification",
        "source_verifier_startup",
        "source_verifier_boot",
    ];
    let protocol_row = |ordinal: usize, protocol: &ManagedCloudProtocolAuthority| {
        Ok(SnapshotProtocolProjectionRow {
            protocol_id: protocol.protocol_id.clone(),
            protocol_version: protocol.protocol_version,
            schema_sha256: protocol.schema_sha256.clone(),
            ordinal: i64::try_from(ordinal)
                .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?,
        })
    };
    let expected_generic = manifest
        .protocols
        .iter()
        .enumerate()
        .filter(|(_, protocol)| !SPECIAL_PROTOCOLS.contains(&protocol.protocol_id.as_str()))
        .map(|(ordinal, protocol)| protocol_row(ordinal, protocol))
        .collect::<ManagedCloudResult<Vec<_>>>()?;
    let expected_special = |id| -> ManagedCloudResult<SnapshotProtocolProjectionRow> {
        let (ordinal, protocol) = manifest
            .protocols
            .iter()
            .enumerate()
            .find(|(_, protocol)| protocol.protocol_id == id)
            .ok_or(ManagedCloudRegistryError::IdentityConflict)?;
        protocol_row(ordinal, protocol)
    };
    let expected_source_verification = if manifest.feature_authority.source_verification {
        Some(expected_special("source_verification")?)
    } else {
        None
    };
    let expected_startup = if matches!(manifest.version, 3 | 4) {
        Some(expected_special("source_verifier_startup")?)
    } else {
        None
    };
    let expected_boot = if matches!(manifest.version, 3 | 4) {
        Some(expected_special("source_verifier_boot")?)
    } else {
        None
    };
    if stored_generic_count
        != i64::try_from(expected_generic.len())
            .map_err(|_| ManagedCloudRegistryError::IdentityConflict)?
        || protocol_rows.generic != expected_generic
        || protocol_rows.source_verification != expected_source_verification
        || protocol_rows.startup != expected_startup
        || protocol_rows.boot != expected_boot
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_snapshot_protocol_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SnapshotProtocolProjectionRow> {
    Ok(SnapshotProtocolProjectionRow {
        protocol_id: row.get(0)?,
        protocol_version: row.get(1)?,
        schema_sha256: row.get(2)?,
        ordinal: row.get(3)?,
    })
}

fn sqlite_snapshot_protocol_rows_v4(
    tx: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> ManagedCloudResult<SnapshotProtocolRows> {
    let mut statement = tx
        .prepare(SQLITE_SNAPSHOT_GENERIC_PROTOCOL_ROWS)
        .map_err(managed_cloud_storage)?;
    let generic = statement
        .query_map(
            rusqlite::params![manifest_sha256],
            sqlite_snapshot_protocol_row,
        )
        .map_err(managed_cloud_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(managed_cloud_storage)?;
    let source_row = |sql| {
        tx.query_row(
            sql,
            rusqlite::params![manifest_sha256],
            sqlite_snapshot_protocol_row,
        )
        .optional()
        .map_err(managed_cloud_storage)
    };
    Ok(SnapshotProtocolRows {
        generic,
        source_verification: source_row(SQLITE_SNAPSHOT_SOURCE_PROTOCOL_ROWS[0])?,
        startup: source_row(SQLITE_SNAPSHOT_SOURCE_PROTOCOL_ROWS[1])?,
        boot: source_row(SQLITE_SNAPSHOT_SOURCE_PROTOCOL_ROWS[2])?,
    })
}

fn postgres_snapshot_protocol_row(row: &postgres::Row) -> SnapshotProtocolProjectionRow {
    SnapshotProtocolProjectionRow {
        protocol_id: row.get(0),
        protocol_version: row.get(1),
        schema_sha256: row.get(2),
        ordinal: row.get(3),
    }
}

fn postgres_snapshot_protocol_rows_v4(
    tx: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> ManagedCloudResult<SnapshotProtocolRows> {
    let generic = tx
        .query(POSTGRES_SNAPSHOT_GENERIC_PROTOCOL_ROWS, &[&manifest_sha256])
        .map_err(managed_cloud_storage)?
        .iter()
        .map(postgres_snapshot_protocol_row)
        .collect();
    let mut source_row = |sql| {
        tx.query_opt(sql, &[&manifest_sha256])
            .map_err(managed_cloud_storage)
            .map(|row| row.map(|row| postgres_snapshot_protocol_row(&row)))
    };
    Ok(SnapshotProtocolRows {
        generic,
        source_verification: source_row(POSTGRES_SNAPSHOT_SOURCE_PROTOCOL_ROWS[0])?,
        startup: source_row(POSTGRES_SNAPSHOT_SOURCE_PROTOCOL_ROWS[1])?,
        boot: source_row(POSTGRES_SNAPSHOT_SOURCE_PROTOCOL_ROWS[2])?,
    })
}

fn sqlite_snapshot_manifest_projection_v4(
    tx: &rusqlite::Transaction<'_>,
    request: &ProvisioningSigningRequest,
    manifest: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    let values = tx
        .query_row(
            &(SNAPSHOT_MANIFEST_PROJECTION.to_owned() + "?1"),
            rusqlite::params![request.manifest_sha256],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                    r.get(14)?,
                ))
            },
        )
        .map_err(managed_cloud_storage)?;
    snapshot_manifest_projection_values_v4(
        values,
        sqlite_snapshot_protocol_rows_v4(tx, &request.manifest_sha256)?,
        manifest,
    )
}

fn postgres_snapshot_manifest_projection_v4(
    tx: &mut postgres::Transaction<'_>,
    request: &ProvisioningSigningRequest,
    manifest: &ManagedCloudReleaseAuthority,
) -> ManagedCloudResult<()> {
    let row = tx
        .query_one(
            &(SNAPSHOT_MANIFEST_PROJECTION.to_owned() + "$1 FOR SHARE"),
            &[&request.manifest_sha256],
        )
        .map_err(managed_cloud_storage)?;
    let values = (
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
    );
    drop(row);
    snapshot_manifest_projection_values_v4(
        values,
        postgres_snapshot_protocol_rows_v4(tx, &request.manifest_sha256)?,
        manifest,
    )
}

fn build_response(
    request: &ProvisioningSigningRequest,
    facts: SnapshotFacts,
) -> ManagedCloudResult<ProvisioningSigningResponse> {
    if request.manifest_sha256 != facts.manifest_sha256
        || request.cohort_sha256 != facts.cohort_sha256
        || request.queue_generation_id != facts.queue_generation_id
        || request.issuance_ref != facts.issuance_ref
        || facts.failure_converter_projection.manifest_sha256 != facts.manifest_sha256
        || !managed_cloud_hex64(&facts.failure_converter_projection.value)
        || request.requested_signed_at_ms < request.issued_at_ms
        || request.requested_signed_at_ms < request.not_before_ms
        || request.requested_signed_at_ms > facts.snapshot_at_ms
        || request.requested_signed_at_ms < facts.policy_valid_from_ms
        || request.requested_signed_at_ms >= facts.policy_expires_at_ms
        || request.requested_signed_at_ms >= request.expires_at_ms
        || request.expires_at_ms > facts.policy_expires_at_ms
        || managed_cloud_required_roles_for_feature(&facts.feature_authority).len() != 9
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }

    let document = ProvisioningDocument {
        version: 4,
        audience: super::managed_cloud_provisioning_persistence::PROVISIONING_AUDIENCE.into(),
        kind: "provisioned_nonserving".into(),
        activation_id: request.activation_id.clone(),
        activation_generation: request.activation_generation,
        trust_generation: facts.trust_generation,
        scope: request.scope.clone(),
        channel_sequence: facts.channel_sequence,
        expected_head_revision: facts.head_revision,
        expected_transition_sha256: facts.previous_transition_sha256.clone(),
        predecessor_activation_sha256: facts.previous_activation_sha256.clone(),
        manifest_sha256: facts.manifest_sha256.clone(),
        manifest_signature_set_sha256: facts.manifest_signature_set_sha256.clone(),
        cohort_sha256: facts.cohort_sha256.clone(),
        cohort_signature_set_sha256: facts.cohort_signature_set_sha256.clone(),
        intended_feature_authority_sha256: managed_cloud_digest(&facts.feature_authority)?,
        intended_feature_authority: facts.feature_authority,
        temporal_namespace_sha256: facts.temporal_namespace_sha256.clone(),
        storage_config_sha256: request.intended_storage_config_sha256.clone(),
        task_queue_sha256: facts.task_queue_sha256.clone(),
        failure_converter_sha256: facts.failure_converter_projection.value.clone(),
        maximum_inflight: request.maximum_inflight,
        maximum_daily_admissions: request.maximum_daily_admissions,
        heartbeat_ttl_ms: request.heartbeat_ttl_ms,
        recovery_acceptances: Vec::new(),
        issued_at_ms: request.issued_at_ms,
        not_before_ms: request.not_before_ms,
        expires_at_ms: request.expires_at_ms,
        authority_binding: facts.authority_binding.clone(),
    };
    if document.authority_binding.authority_kind != AuthorityKind::ProvisioningV4
        || document.authority_binding.purpose != Purpose::Production
        || document.authority_binding.context_sha256.is_some()
        || document.authority_binding.queue_generation_id != facts.queue_generation_id
    {
        return Err(ManagedCloudRegistryError::InvalidAuthority);
    }

    let document_bytes = managed_cloud_canonical_json(&document)?;
    let document_sha256 = managed_cloud_sha256(&document_bytes);
    let unsigned = UnsignedSignatureSet {
        version: 1,
        audience: "bluey-jobs-managed-cloud-signature-set-v1".into(),
        signature_set_id: request.signature_set_id.clone(),
        trust_generation: facts.trust_generation,
        role: facts.signing_role.clone(),
        target_audience: super::managed_cloud_provisioning_persistence::PROVISIONING_AUDIENCE
            .into(),
        target_sha256: document_sha256.clone(),
        signed_at_ms: request.requested_signed_at_ms,
    };
    let payload = managed_cloud_canonical_json(&unsigned)?;

    Ok(ProvisioningSigningResponse {
        version: SNAPSHOT_VERSION,
        snapshot_at_ms: facts.snapshot_at_ms,
        document_canonical_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(document_bytes),
        document_sha256: document_sha256.clone(),
        unsigned_signature_set: unsigned,
        signature_payload_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(payload),
        sources: SnapshotSources {
            storage_config_sha256: IntentDigestSource {
                value: request.intended_storage_config_sha256.clone(),
                origin: ADMIN_INTENT.into(),
            },
            failure_converter_sha256: IntentDigestSource {
                value: facts.failure_converter_projection.value.clone(),
                origin: VALIDATED_IMPORT_PROJECTION.into(),
            },
            manifest: DATABASE_SNAPSHOT.into(),
            cohort: DATABASE_SNAPSHOT.into(),
            trust: DATABASE_SNAPSHOT.into(),
            queue: DATABASE_SNAPSHOT.into(),
            head: DATABASE_SNAPSHOT.into(),
        },
        expected_head: ExpectedHead {
            revision: facts.head_revision,
            transition_sha256: facts.previous_transition_sha256,
            activation_sha256: facts.previous_activation_sha256,
        },
        queue: SnapshotQueue {
            queue_generation_id: facts.queue_generation_id,
            issuance_ref: facts.issuance_ref,
            task_queue_sha256: facts.task_queue_sha256,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ProvisioningSigningRequest {
        ProvisioningSigningRequest {
            version: SNAPSHOT_VERSION,
            scope: ManagedCloudScope {
                environment: "production".into(),
                region: "us-east-1".into(),
                channel: "general".into(),
            },
            queue_generation_id: "queue-generation-id-0001".into(),
            issuance_ref: "issuance-reference-id-0001".into(),
            manifest_sha256: "1".repeat(64),
            cohort_sha256: "2".repeat(64),
            activation_id: "activation-id-0001".into(),
            activation_generation: 1,
            issued_at_ms: 100,
            not_before_ms: 100,
            expires_at_ms: 2_000,
            maximum_inflight: 1,
            maximum_daily_admissions: 1,
            heartbeat_ttl_ms: 5_000,
            intended_storage_config_sha256: "3".repeat(64),
            signature_set_id: "signature-set-id-0001".into(),
            requested_signed_at_ms: 1_000,
        }
    }

    fn facts(request: &ProvisioningSigningRequest) -> SnapshotFacts {
        SnapshotFacts {
            snapshot_at_ms: 1_000,
            head_revision: 0,
            previous_transition_sha256: None,
            previous_activation_sha256: None,
            trust_generation: 1,
            channel_sequence: 1,
            manifest_sha256: request.manifest_sha256.clone(),
            cohort_sha256: request.cohort_sha256.clone(),
            manifest_signature_set_sha256: "4".repeat(64),
            cohort_signature_set_sha256: "5".repeat(64),
            feature_authority: ManagedCloudFeatureAuthority {
                cloud_distribution: false,
                workflow_command_dispatch: false,
                workflow_cleanup: false,
                direct_discovery: true,
                global_discovery: true,
                source_verification: true,
            },
            temporal_namespace_sha256: "6".repeat(64),
            task_queue_sha256: "7".repeat(64),
            failure_converter_projection: ValidatedImportFailureConverterProjection {
                manifest_sha256: request.manifest_sha256.clone(),
                value: "8".repeat(64),
            },
            authority_binding: AuthorityBinding {
                authority_kind: AuthorityKind::ProvisioningV4,
                purpose: Purpose::Production,
                context_sha256: None,
                queue_generation_id: request.queue_generation_id.clone(),
            },
            signing_role: "general_promotion".into(),
            policy_valid_from_ms: 10,
            policy_expires_at_ms: 3_000,
            queue_generation_id: request.queue_generation_id.clone(),
            issuance_ref: request.issuance_ref.clone(),
        }
    }

    #[test]
    fn request_is_closed_and_requires_lower_hex_storage_intent() {
        let json = r#"{
            "version":1,
            "scope":{"environment":"production","region":"us-east-1","channel":"general"},
            "queueGenerationId":"queue-generation-id-0001",
            "issuanceRef":"issuance-reference-id-0001",
            "manifestSha256":"1111111111111111111111111111111111111111111111111111111111111111",
            "cohortSha256":"2222222222222222222222222222222222222222222222222222222222222222",
            "activationId":"activation-id-0001",
            "activationGeneration":1,
            "issuedAtMs":100,
            "notBeforeMs":100,
            "expiresAtMs":2000,
            "maximumInflight":1,
            "maximumDailyAdmissions":1,
            "heartbeatTtlMs":5000,
            "intendedStorageConfigSha256":"3333333333333333333333333333333333333333333333333333333333333333",
            "signatureSetId":"signature-set-id-0001",
            "requestedSignedAtMs":1000
        }"#;
        let parsed: ProvisioningSigningRequest = serde_json::from_str(json).unwrap();
        validate_request(&parsed, "phase656-unit").unwrap();
        let mut extra: serde_json::Value = serde_json::from_str(json).unwrap();
        extra
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ProvisioningSigningRequest>(extra).is_err());
        let mut malformed = request();
        malformed.intended_storage_config_sha256 = "A".repeat(64);
        assert!(validate_request(&malformed, "phase656-unit").is_err());
    }

    #[test]
    fn channel_limits_match_the_activation_schema_for_every_channel() {
        let valid = [
            ("shadow", 0, 0),
            ("canary", 1, 1),
            ("canary", 100_000, 1_000_000),
            ("general", 1, 1),
            ("general", 100_000, 1_000_000),
        ];
        for (channel, maximum_inflight, maximum_daily_admissions) in valid {
            let mut candidate = request();
            candidate.scope.channel = channel.into();
            candidate.maximum_inflight = maximum_inflight;
            candidate.maximum_daily_admissions = maximum_daily_admissions;
            assert!(
                validate_request(&candidate, "phase656-unit").is_ok(),
                "expected {channel} {maximum_inflight}/{maximum_daily_admissions} to pass"
            );
        }

        let invalid = [
            ("shadow", 1, 0),
            ("shadow", 0, 1),
            ("shadow", 1, 1),
            ("canary", 0, 0),
            ("canary", 0, 1),
            ("canary", 1, 0),
            ("canary", 100_001, 1),
            ("canary", 1, 1_000_001),
            ("general", 0, 0),
            ("general", 0, 1),
            ("general", 1, 0),
            ("general", 100_001, 1),
            ("general", 1, 1_000_001),
        ];
        for (channel, maximum_inflight, maximum_daily_admissions) in invalid {
            let mut candidate = request();
            candidate.scope.channel = channel.into();
            candidate.maximum_inflight = maximum_inflight;
            candidate.maximum_daily_admissions = maximum_daily_admissions;
            assert!(
                validate_request(&candidate, "phase656-unit").is_err(),
                "expected {channel} {maximum_inflight}/{maximum_daily_admissions} to fail"
            );
        }
    }

    #[test]
    fn admin_storage_intent_changes_unsigned_document_and_payload_without_signature() {
        let first_request = request();
        let first = build_response(&first_request, facts(&first_request)).unwrap();
        let mut second_request = request();
        second_request.intended_storage_config_sha256 = "9".repeat(64);
        let second = build_response(&second_request, facts(&second_request)).unwrap();

        assert_ne!(first.document_sha256, second.document_sha256);
        assert_ne!(
            first.signature_payload_base64url,
            second.signature_payload_base64url
        );
        assert_eq!(first.sources.storage_config_sha256.origin, ADMIN_INTENT);
        assert_eq!(first.sources.manifest, DATABASE_SNAPSHOT);
        assert_eq!(first.sources.head, DATABASE_SNAPSHOT);

        let document_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&first.document_canonical_base64url)
            .unwrap();
        let document: ProvisioningDocument =
            managed_cloud_parse_canonical(&document_bytes).unwrap();
        assert_eq!(
            document.storage_config_sha256,
            first_request.intended_storage_config_sha256
        );
        let unsigned = serde_json::to_value(&first.unsigned_signature_set).unwrap();
        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&first.signature_payload_base64url)
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert!(unsigned.get("signatures").is_none());
        assert!(payload.get("signatures").is_none());
    }

    #[test]
    fn failure_converter_projection_is_manifest_bound_and_explicitly_labeled() {
        let request = request();
        let response = build_response(&request, facts(&request)).unwrap();
        assert_eq!(
            response.sources.failure_converter_sha256.value,
            "8".repeat(64)
        );
        assert_eq!(
            response.sources.failure_converter_sha256.origin,
            VALIDATED_IMPORT_PROJECTION
        );

        let document_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&response.document_canonical_base64url)
            .unwrap();
        let document: ProvisioningDocument =
            managed_cloud_parse_canonical(&document_bytes).unwrap();
        assert_eq!(document.failure_converter_sha256, "8".repeat(64));

        let mut changed_projection = facts(&request);
        changed_projection.failure_converter_projection.value = "a".repeat(64);
        let changed = build_response(&request, changed_projection).unwrap();
        assert_ne!(response.document_sha256, changed.document_sha256);
        assert_ne!(
            response.signature_payload_base64url,
            changed.signature_payload_base64url
        );

        let mut wrong_manifest = facts(&request);
        wrong_manifest.failure_converter_projection.manifest_sha256 = "9".repeat(64);
        assert!(build_response(&request, wrong_manifest).is_err());

        let mut malformed_value = facts(&request);
        malformed_value.failure_converter_projection.value = "A".repeat(64);
        assert!(build_response(&request, malformed_value).is_err());
    }
}

// Insert as a cfg(test) child of managed_cloud_provisioning_snapshot.rs.
#[cfg(test)]
pub(crate) mod sqlite_snapshot_tests {
    use super::super::managed_cloud_provisioning_installation::{
        allocate_provisioning_queue, AllocateProvisioningQueueRequest,
    };
    use super::super::managed_cloud_release_authority_tests as legacy_tests;
    use super::*;
    use base64::Engine;
    use serial_test::serial;
    use std::ffi::OsString;

    pub(crate) struct Seed {
        pool: DbPool,
        pub(crate) request: ProvisioningSigningRequest,
        pub(crate) task_queue_sha256: String,
        _root: RootAnchorEnvGuard,
    }

    struct RootAnchorEnvGuard(Option<OsString>, Option<OsString>);
    impl RootAnchorEnvGuard {
        fn install(anchor: &ManagedCloudRootTrustAnchor) -> Self {
            let json = String::from_utf8(managed_cloud_canonical_json(anchor).unwrap()).unwrap();
            let guard = Self(
                std::env::var_os(MANAGED_CLOUD_ROOT_TRUST_ANCHOR_ENV),
                std::env::var_os(MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256_ENV),
            );
            std::env::set_var(MANAGED_CLOUD_ROOT_TRUST_ANCHOR_ENV, &json);
            std::env::set_var(
                MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256_ENV,
                managed_cloud_sha256(json.as_bytes()),
            );
            guard
        }
    }
    impl Drop for RootAnchorEnvGuard {
        fn drop(&mut self) {
            for (name, value) in [
                (MANAGED_CLOUD_ROOT_TRUST_ANCHOR_ENV, self.0.take()),
                (MANAGED_CLOUD_ROOT_TRUST_ANCHOR_SHA256_ENV, self.1.take()),
            ] {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    pub(crate) fn seed_for_http(pool: DbPool) -> Seed {
        let now = crate::db::run_blocking_db(|| {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
        });
        seed_authority(pool, now)
    }

    pub(crate) fn seed_authority(pool: DbPool, now: i64) -> Seed {
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
            policy_id: "snapshot-sqlite-policy".into(),
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
        let root_guard = RootAnchorEnvGuard::install(&root);
        let envelope = legacy_tests::managed_cloud_test_signed_envelope(
            &policy,
            MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
            "root",
            now - 1_000,
        );
        import_managed_cloud_trust_policy_with_bootstrap_anchor(
            &pool,
            &envelope.envelope,
            "snapshot-sqlite",
            Some(&root),
        )
        .unwrap();
        let (_release, release_request) =
            legacy_tests::managed_cloud_test_release_import(issued, 3);
        let manifest =
            import_managed_cloud_release(&pool, &release_request, "snapshot-sqlite").unwrap();
        let scope = ManagedCloudScope {
            environment: "production".into(),
            region: "us-east-1".into(),
            channel: "general".into(),
        };
        let cohort = ManagedCloudCohortAuthority {
            version: 1,
            audience: MANAGED_CLOUD_COHORT_AUDIENCE.into(),
            cohort_id: "snapshot-sqlite-cohort".into(),
            cohort_generation: 1,
            trust_generation: 1,
            scope: scope.clone(),
            rollout_mode: "all_eligible_accounts".into(),
            account_id_sha256s: vec![],
            approval_ref: "snapshot-sqlite-approval".into(),
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
            &pool,
            &ManagedCloudCohortImportRequest {
                envelope: cohort_envelope.envelope,
                account_ids: vec![],
            },
            "snapshot-sqlite",
        )
        .unwrap();
        let queue = allocate_provisioning_queue(
            &pool,
            &ProvisioningProcessContext::production(),
            &AllocateProvisioningQueueRequest {
                issuance_ref: "snapshot-sqlite-issuance".into(),
                scope: scope.clone(),
                namespace: "snapshot-sqlite-namespace".into(),
                task_queue: "snapshot-sqlite-queue".into(),
            },
            "snapshot-sqlite",
        )
        .unwrap();
        let task_queue_sha256 = queue.task_queue_sha256.clone();
        Seed {
            request: ProvisioningSigningRequest {
                version: 1,
                scope,
                queue_generation_id: queue.queue_generation_id,
                issuance_ref: queue.issuance_ref,
                manifest_sha256: manifest.authority_sha256,
                cohort_sha256: imported_cohort.authority_sha256,
                activation_id: "snapshot-sqlite-activation".into(),
                activation_generation: 1,
                issued_at_ms: issued,
                not_before_ms: issued,
                expires_at_ms: expires,
                maximum_inflight: 1,
                maximum_daily_admissions: 1,
                heartbeat_ttl_ms: 5_000,
                intended_storage_config_sha256: "a".repeat(64),
                signature_set_id: "snapshot-sqlite-signature-set".into(),
                requested_signed_at_ms: now - 500,
            },
            task_queue_sha256,
            pool,
            _root: root_guard,
        }
    }

    fn seed() -> Seed {
        seed_for_http(super::super::tests::test_pool())
    }

    pub(crate) fn assert_http_document(
        bytes: &[u8],
        request: &ProvisioningSigningRequest,
        task_queue_sha256: &str,
        document_sha256: &str,
    ) {
        assert_eq!(managed_cloud_sha256(bytes), document_sha256);
        let document: ProvisioningDocument = managed_cloud_parse_canonical(bytes).unwrap();
        assert_eq!(managed_cloud_canonical_json(&document).unwrap(), bytes);
        assert_eq!(document.channel_sequence, 1);
        assert_eq!(document.expected_head_revision, 0);
        assert_eq!(document.expected_transition_sha256, None);
        assert_eq!(document.predecessor_activation_sha256, None);
        assert_eq!(document.scope, request.scope);
        assert_eq!(document.manifest_sha256, request.manifest_sha256);
        assert_eq!(document.cohort_sha256, request.cohort_sha256);
        assert_eq!(document.task_queue_sha256, task_queue_sha256);
        assert_eq!(document.activation_id, request.activation_id);
        assert_eq!(
            document.activation_generation,
            request.activation_generation
        );
        assert_eq!(
            document.storage_config_sha256,
            request.intended_storage_config_sha256
        );
        assert_eq!(
            document.authority_binding.authority_kind,
            AuthorityKind::ProvisioningV4
        );
        assert_eq!(document.authority_binding.purpose, Purpose::Production);
        assert_eq!(document.authority_binding.context_sha256, None);
        assert_eq!(
            document.authority_binding.queue_generation_id,
            request.queue_generation_id
        );
    }

    #[test]
    #[serial]
    fn sqlite_snapshot_null_head_returns_unsigned_sequence_one() {
        let seed = seed();
        let before = capture_durable_sqlite(&seed.pool).unwrap();
        let result = snapshot_provisioning_signing_request(
            &seed.pool,
            &ProvisioningProcessContext::production(),
            &seed.request,
            "snapshot-sqlite",
        )
        .unwrap();
        assert_eq!(result.expected_head.revision, 0);
        assert_eq!(result.expected_head.transition_sha256, None);
        assert_eq!(result.expected_head.activation_sha256, None);
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&result.document_canonical_base64url)
            .unwrap();
        assert_eq!(managed_cloud_sha256(&bytes), result.document_sha256);
        assert_eq!(
            result.unsigned_signature_set.target_sha256,
            result.document_sha256
        );
        let document: ProvisioningDocument = managed_cloud_parse_canonical(&bytes).unwrap();
        assert_eq!(managed_cloud_canonical_json(&document).unwrap(), bytes);
        assert_eq!(document.channel_sequence, 1);
        assert_eq!(document.expected_head_revision, 0);
        assert_eq!(document.expected_transition_sha256, None);
        assert_eq!(document.predecessor_activation_sha256, None);
        assert_eq!(document.activation_id, seed.request.activation_id);
        assert_eq!(
            document.activation_generation,
            seed.request.activation_generation
        );
        assert_eq!(document.manifest_sha256, seed.request.manifest_sha256);
        assert_eq!(document.cohort_sha256, seed.request.cohort_sha256);
        assert_eq!(
            document.storage_config_sha256,
            seed.request.intended_storage_config_sha256
        );
        assert_eq!(document.scope, seed.request.scope);
        assert_eq!(
            document.authority_binding.authority_kind,
            AuthorityKind::ProvisioningV4
        );
        assert_eq!(document.authority_binding.purpose, Purpose::Production);
        assert_eq!(document.authority_binding.context_sha256, None);
        assert_eq!(
            document.authority_binding.queue_generation_id,
            seed.request.queue_generation_id
        );
        assert_eq!(
            result.queue.queue_generation_id,
            seed.request.queue_generation_id
        );
        assert_eq!(result.queue.issuance_ref, seed.request.issuance_ref);
        assert_eq!(result.queue.task_queue_sha256, document.task_queue_sha256);
        assert_eq!(
            result.sources.storage_config_sha256.value,
            seed.request.intended_storage_config_sha256
        );
        assert_eq!(
            result.sources.failure_converter_sha256.value,
            document.failure_converter_sha256
        );
        assert_eq!(result.sources.storage_config_sha256.origin, "admin_intent");
        assert_eq!(
            result.sources.failure_converter_sha256.origin,
            "validated_import_projection"
        );
        let after = capture_durable_sqlite(&seed.pool).unwrap();
        assert_eq!(after, before);
    }

    #[test]
    #[serial]
    fn postgres_snapshot_null_head_canonical_and_unknown_manifest_when_owned() {
        const CASE: &str = "db::jobs::managed_cloud_provisioning_snapshot::sqlite_snapshot_tests::postgres_snapshot_null_head_canonical_and_unknown_manifest_when_owned";
        let Some(url) = crate::db::owned_postgres_test::owned_url(CASE) else {
            return;
        };
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        let fresh: bool = pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(
            fresh,
            "owned PostgreSQL case must begin with a fresh database"
        );
        crate::db::run_migrations(&pool).unwrap();

        let now = crate::db::run_blocking_db(|| {
            pool.get_pg()
                .unwrap()
                .query_one(
                    "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )
                .unwrap()
                .get::<_, i64>(0)
        });
        let mut seed = seed_authority(pool, now);
        let result = snapshot_provisioning_signing_request(
            &seed.pool,
            &ProvisioningProcessContext::production(),
            &seed.request,
            "snapshot-postgres",
        )
        .unwrap();
        assert_eq!(result.expected_head.revision, 0);
        assert_eq!(result.expected_head.transition_sha256, None);
        assert_eq!(result.expected_head.activation_sha256, None);
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&result.document_canonical_base64url)
            .unwrap();
        assert_eq!(managed_cloud_sha256(&bytes), result.document_sha256);
        assert_eq!(
            result.unsigned_signature_set.target_sha256,
            result.document_sha256
        );
        assert_http_document(
            &bytes,
            &seed.request,
            &seed.task_queue_sha256,
            &result.document_sha256,
        );
        assert_eq!(
            result.queue.queue_generation_id,
            seed.request.queue_generation_id
        );
        assert_eq!(result.queue.issuance_ref, seed.request.issuance_ref);
        assert_eq!(result.queue.task_queue_sha256, seed.task_queue_sha256);

        let original_manifest_sha256 = seed.request.manifest_sha256.clone();
        seed.request.manifest_sha256 = "f".repeat(64);
        assert_ne!(seed.request.manifest_sha256, original_manifest_sha256);
        let missing_count: i64 = seed
            .pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT COUNT(*) FROM jobs_managed_cloud_manifests WHERE manifest_sha256=$1",
                &[&seed.request.manifest_sha256],
            )
            .unwrap()
            .get(0);
        assert_eq!(missing_count, 0);
        assert!(matches!(
            snapshot_provisioning_signing_request(
                &seed.pool,
                &ProvisioningProcessContext::production(),
                &seed.request,
                "snapshot-postgres",
            ),
            Err(ManagedCloudRegistryError::NotFound)
        ));
        println!("phase657_owned_postgres_provisioning_snapshot_canonical_not_found_complete");
        println!("phase657_owned_postgres_snapshot_null_head_complete");
    }

    #[test]
    #[serial]
    fn sqlite_snapshot_unknown_manifest_refuses_not_found() {
        let mut seed = seed();
        let original_manifest_sha256 = seed.request.manifest_sha256.clone();
        seed.request.manifest_sha256 = "f".repeat(64);
        assert_ne!(seed.request.manifest_sha256, original_manifest_sha256);
        let count: i64 = crate::db::run_blocking_db(|| {
            seed.pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_managed_cloud_manifests WHERE manifest_sha256=?1",
                    [&seed.request.manifest_sha256],
                    |row| row.get(0),
                )
                .unwrap()
        });
        assert_eq!(count, 0);
        let before = capture_durable_sqlite(&seed.pool).unwrap();
        let result = snapshot_provisioning_signing_request(
            &seed.pool,
            &ProvisioningProcessContext::production(),
            &seed.request,
            "snapshot-sqlite",
        );
        assert!(matches!(result, Err(ManagedCloudRegistryError::NotFound)));
        let after = capture_durable_sqlite(&seed.pool).unwrap();
        assert_eq!(after, before);
    }
}
