//! C1 kind-aware storage projection. No provisioning API writer or proof acceptance.
//! Context/allocation rows are metadata, never evidence of resource/Temporal ownership.

use super::managed_cloud_provisioning_protocol::{
    canonical_binding_bytes, AuthorityBinding, Purpose,
};
use super::{
    managed_cloud_decode_base64url, managed_cloud_digest, managed_cloud_hex64,
    managed_cloud_parse_canonical, managed_cloud_required_roles_for_feature,
    managed_cloud_safe_integer, managed_cloud_sha256, managed_cloud_storage,
    ManagedCloudFeatureAuthority, ManagedCloudRecoveryAcceptanceAuthority,
    ManagedCloudRegistryError, ManagedCloudResolvedHead, ManagedCloudResult, ManagedCloudScope,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

pub(super) const PROVISIONING_AUDIENCE: &str = "bluey-jobs-managed-cloud-provisioning-v4";
pub(super) const CONTEXT_AUDIENCE: &str = "bluey-jobs-managed-cloud-preproduction-context-v1";
pub(super) const LATCH_AUDIENCE: &str = "bluey-jobs-managed-cloud-serving-latch-v1";

#[derive(Debug, Clone)]
pub(super) enum StoredAuthority {
    LegacyActivation,
    ProvisioningV4(Box<ProvisioningDocument>),
}

#[derive(Clone)]
pub(super) struct StoredAuthorityColumns {
    pub(super) kind: String,
    pub(super) canonical: Option<String>,
    pub(super) purpose: String,
    pub(super) context_sha256: Option<String>,
    pub(super) queue_generation_id: Option<String>,
    pub(super) feature_authority_sha256: String,
    pub(super) activation_id: String,
    pub(super) activation_generation: i64,
    pub(super) manifest_signature_set_sha256: String,
    pub(super) cohort_signature_set_sha256: String,
    pub(super) temporal_namespace_sha256: String,
    pub(super) storage_config_sha256: String,
    pub(super) maximum_inflight: i64,
    pub(super) maximum_daily_admissions: i64,
    pub(super) heartbeat_ttl_ms: i64,
    pub(super) issued_at_ms: i64,
    pub(super) not_before_ms: i64,
    pub(super) recovery_acceptance_count: i64,
    pub(super) requirement_count: i64,
    pub(super) expected_head_revision: i64,
    pub(super) expected_transition_sha256: Option<String>,
    pub(super) predecessor_activation_sha256: Option<String>,
    pub(super) previous_head_revision: i64,
    pub(super) previous_transition_sha256: Option<String>,
    pub(super) previous_activation_sha256: Option<String>,
    // Read the actual head tuple separately: legacy metadata keeps its original
    // activation-derived projection, while v4 requires all three representations equal.
    pub(super) head_trust_generation: i64,
    pub(super) head_channel_sequence: i64,
}

#[derive(Clone)]
pub(super) struct StoredHeadMetadata {
    pub(super) scope: ManagedCloudScope,
    pub(super) head_revision: i64,
    pub(super) transition_sha256: String,
    pub(super) activation_sha256: String,
    pub(super) manifest_sha256: String,
    pub(super) cohort_sha256: String,
    pub(super) trust_generation: i64,
    pub(super) channel_sequence: i64,
    pub(super) release_id: String,
    pub(super) release_sequence: i64,
    pub(super) task_queue_sha256: String,
    pub(super) failure_converter_sha256: String,
    pub(super) activation_expires_at_ms: i64,
    pub(super) features: ManagedCloudFeatureAuthority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProvisioningDocument {
    pub(super) version: i64,
    pub(super) audience: String,
    pub(super) kind: String,
    pub(super) activation_id: String,
    pub(super) activation_generation: i64,
    pub(super) trust_generation: i64,
    pub(super) scope: ManagedCloudScope,
    pub(super) channel_sequence: i64,
    pub(super) expected_head_revision: i64,
    pub(super) expected_transition_sha256: Option<String>,
    pub(super) predecessor_activation_sha256: Option<String>,
    pub(super) manifest_sha256: String,
    pub(super) manifest_signature_set_sha256: String,
    pub(super) cohort_sha256: String,
    pub(super) cohort_signature_set_sha256: String,
    pub(super) intended_feature_authority_sha256: String,
    pub(super) intended_feature_authority: ManagedCloudFeatureAuthority,
    pub(super) temporal_namespace_sha256: String,
    pub(super) storage_config_sha256: String,
    pub(super) task_queue_sha256: String,
    pub(super) failure_converter_sha256: String,
    pub(super) maximum_inflight: i64,
    pub(super) maximum_daily_admissions: i64,
    pub(super) heartbeat_ttl_ms: i64,
    pub(super) recovery_acceptances: Vec<ManagedCloudRecoveryAcceptanceAuthority>,
    pub(super) issued_at_ms: i64,
    pub(super) not_before_ms: i64,
    pub(super) expires_at_ms: i64,
    pub(super) authority_binding: AuthorityBinding,
}

impl ProvisioningDocument {
    // Only the kind-first, relationally checked projection constructs this value.
    // These control-plane facts do not assert physical qualification or serving.
    pub(super) fn control_binding(&self) -> &AuthorityBinding {
        &self.authority_binding
    }
    pub(super) fn control_ttl_ms(&self) -> i64 {
        self.heartbeat_ttl_ms
    }
    pub(super) fn control_not_before_ms(&self) -> i64 {
        self.not_before_ms
    }
    pub(super) fn control_issued_at_ms(&self) -> i64 {
        self.issued_at_ms
    }
    pub(super) fn control_namespace_sha256(&self) -> &str {
        &self.temporal_namespace_sha256
    }
}

fn project_stored_authority(
    head: &StoredHeadMetadata,
    columns: StoredAuthorityColumns,
) -> ManagedCloudResult<StoredAuthority> {
    match columns.kind.as_str() {
        "legacy_activation" => {
            if columns.purpose != "production"
                || columns.context_sha256.is_some()
                || columns.queue_generation_id.is_some()
            {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
            // Legacy business readers keep their existing canonical validators.
            Ok(StoredAuthority::LegacyActivation)
        }
        "provisioning_v4" => {
            let canonical = columns
                .canonical
                .ok_or(ManagedCloudRegistryError::InvalidAuthority)?;
            let bytes = managed_cloud_decode_base64url(&canonical)?;
            let document: ProvisioningDocument = managed_cloud_parse_canonical(&bytes)?;
            canonical_binding_bytes(&document.authority_binding)?;
            let expected_purpose = match document.authority_binding.purpose {
                Purpose::Production => "production",
                Purpose::NonproductionSynthetic => "nonproduction_synthetic",
            };
            let inert = ManagedCloudFeatureAuthority {
                cloud_distribution: false,
                workflow_command_dispatch: false,
                workflow_cleanup: false,
                direct_discovery: false,
                global_discovery: false,
                source_verification: false,
            };
            let valid_hashes = [
                &document.manifest_signature_set_sha256,
                &document.cohort_signature_set_sha256,
                &document.temporal_namespace_sha256,
                &document.storage_config_sha256,
            ]
            .into_iter()
            .all(|value| managed_cloud_hex64(value));
            if document.version != 4
                || document.audience != PROVISIONING_AUDIENCE
                || document.kind != "provisioned_nonserving"
                || !super::managed_cloud_token(&document.activation_id, 128)
                || document.activation_id != columns.activation_id
                || document.activation_generation != columns.activation_generation
                || !managed_cloud_safe_integer(document.activation_generation, true)
                || document.scope != head.scope
                || document.manifest_sha256 != head.manifest_sha256
                || document.cohort_sha256 != head.cohort_sha256
                || document.trust_generation != head.trust_generation
                || document.channel_sequence != head.channel_sequence
                || document.trust_generation != columns.head_trust_generation
                || document.channel_sequence != columns.head_channel_sequence
                || document.expected_head_revision != head.head_revision - 1
                || document.expected_head_revision != columns.expected_head_revision
                || document.expected_head_revision != columns.previous_head_revision
                || document.expected_transition_sha256 != columns.expected_transition_sha256
                || document.expected_transition_sha256 != columns.previous_transition_sha256
                || document.predecessor_activation_sha256 != columns.predecessor_activation_sha256
                || document.predecessor_activation_sha256 != columns.previous_activation_sha256
                || (document.expected_head_revision == 0)
                    != document.expected_transition_sha256.is_none()
                || (document.expected_head_revision == 0)
                    != document.predecessor_activation_sha256.is_none()
                || document
                    .expected_transition_sha256
                    .as_ref()
                    .is_some_and(|value| !managed_cloud_hex64(value))
                || document
                    .predecessor_activation_sha256
                    .as_ref()
                    .is_some_and(|value| !managed_cloud_hex64(value))
                || document.task_queue_sha256 != head.task_queue_sha256
                || document.failure_converter_sha256 != head.failure_converter_sha256
                || document.expires_at_ms != head.activation_expires_at_ms
                || document.manifest_signature_set_sha256 != columns.manifest_signature_set_sha256
                || document.cohort_signature_set_sha256 != columns.cohort_signature_set_sha256
                || document.temporal_namespace_sha256 != columns.temporal_namespace_sha256
                || document.storage_config_sha256 != columns.storage_config_sha256
                || document.maximum_inflight != columns.maximum_inflight
                || document.maximum_daily_admissions != columns.maximum_daily_admissions
                || document.heartbeat_ttl_ms != columns.heartbeat_ttl_ms
                || document.issued_at_ms != columns.issued_at_ms
                || document.not_before_ms != columns.not_before_ms
                || document.recovery_acceptances.len() as i64 != columns.recovery_acceptance_count
                || managed_cloud_required_roles_for_feature(&document.intended_feature_authority)
                    .len() as i64
                    != columns.requirement_count
                || ![
                    document.issued_at_ms,
                    document.not_before_ms,
                    document.expires_at_ms,
                ]
                .into_iter()
                .all(|value| managed_cloud_safe_integer(value, false))
                || document.not_before_ms < document.issued_at_ms
                || document.expires_at_ms <= document.not_before_ms
                || !(0..=100_000).contains(&document.maximum_inflight)
                || !(0..=1_000_000).contains(&document.maximum_daily_admissions)
                || !(5_000..=300_000).contains(&document.heartbeat_ttl_ms)
                || !document.recovery_acceptances.is_empty()
                || document.intended_feature_authority_sha256 != columns.feature_authority_sha256
                || managed_cloud_digest(&document.intended_feature_authority)?
                    != columns.feature_authority_sha256
                || managed_cloud_required_roles_for_feature(&document.intended_feature_authority)
                    .len()
                    != 9
                || head.features != inert
                || columns.purpose != expected_purpose
                || document.authority_binding.context_sha256 != columns.context_sha256
                || Some(&document.authority_binding.queue_generation_id)
                    != columns.queue_generation_id.as_ref()
                || managed_cloud_sha256(&bytes) != head.activation_sha256
                || !valid_hashes
            {
                return Err(ManagedCloudRegistryError::InvalidAuthority);
            }
            // A typed document is metadata, NOT signature/resource/queue proof or serving permission.
            Ok(StoredAuthority::ProvisioningV4(Box::new(document)))
        }
        _ => Err(ManagedCloudRegistryError::InvalidAuthority),
    }
}

pub(super) fn project_stored_head(
    head: StoredHeadMetadata,
    columns: StoredAuthorityColumns,
) -> ManagedCloudResult<ManagedCloudResolvedHead> {
    let authority = project_stored_authority(&head, columns)?;
    Ok(ManagedCloudResolvedHead {
        authority,
        scope: head.scope,
        head_revision: head.head_revision,
        transition_sha256: head.transition_sha256,
        activation_sha256: head.activation_sha256,
        manifest_sha256: head.manifest_sha256,
        cohort_sha256: head.cohort_sha256,
        trust_generation: head.trust_generation,
        channel_sequence: head.channel_sequence,
        release_id: head.release_id,
        release_sequence: head.release_sequence,
        task_queue_sha256: head.task_queue_sha256,
        failure_converter_sha256: head.failure_converter_sha256,
        activation_expires_at_ms: head.activation_expires_at_ms,
        features: head.features,
    })
}

pub(super) fn is_provisioning(authority: &StoredAuthority) -> bool {
    match authority {
        StoredAuthority::LegacyActivation => false,
        StoredAuthority::ProvisioningV4(document) => document.version == 4,
    }
}

pub(super) fn require_sqlite_legacy_activation(
    tx: &rusqlite::Transaction<'_>,
    activation_sha256: &str,
) -> ManagedCloudResult<()> {
    let kind = tx
        .query_row(
            "SELECT authority_kind FROM jobs_managed_cloud_activations WHERE activation_sha256=?1",
            [activation_sha256],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    if kind
        .as_deref()
        .is_some_and(|kind| kind != "legacy_activation")
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

pub(super) fn require_postgres_legacy_activation(
    tx: &mut postgres::Transaction<'_>,
    activation_sha256: &str,
) -> ManagedCloudResult<()> {
    let kind = tx
        .query_opt(
            "SELECT authority_kind FROM jobs_managed_cloud_activations WHERE activation_sha256=$1",
            &[&activation_sha256],
        )
        .map_err(managed_cloud_storage)?
        .map(|row| row.get::<_, String>(0));
    if kind
        .as_deref()
        .is_some_and(|kind| kind != "legacy_activation")
    {
        return Err(ManagedCloudRegistryError::IdentityConflict);
    }
    Ok(())
}

pub(super) fn require_sqlite_legacy_scope_head(
    tx: &rusqlite::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<()> {
    let kind = tx
        .query_row(
            "SELECT activation.authority_kind FROM jobs_managed_cloud_heads head
         JOIN jobs_managed_cloud_activations activation
           ON activation.activation_sha256=head.current_activation_sha256
         WHERE head.environment=?1 AND head.region=?2 AND head.channel=?3",
            rusqlite::params![scope.environment, scope.region, scope.channel],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(managed_cloud_storage)?;
    if kind
        .as_deref()
        .is_some_and(|kind| kind != "legacy_activation")
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

pub(super) fn require_postgres_legacy_scope_head(
    tx: &mut postgres::Transaction<'_>,
    scope: &ManagedCloudScope,
) -> ManagedCloudResult<()> {
    let kind = tx
        .query_opt(
            "SELECT activation.authority_kind FROM jobs_managed_cloud_heads head
         JOIN jobs_managed_cloud_activations activation
           ON activation.activation_sha256=head.current_activation_sha256
         WHERE head.environment=$1 AND head.region=$2 AND head.channel=$3",
            &[&scope.environment, &scope.region, &scope.channel],
        )
        .map_err(managed_cloud_storage)?
        .map(|row| row.get::<_, String>(0));
    if kind
        .as_deref()
        .is_some_and(|kind| kind != "legacy_activation")
    {
        return Err(ManagedCloudRegistryError::Unavailable);
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::db::{self, jobs, DbPool};
    use base64::Engine;
    use ed25519_dalek::Signer;
    use jobs::managed_cloud_provisioning_protocol::AuthorityKind;
    use jobs::managed_cloud_release_authority_tests as legacy_tests;

    struct ProvisioningTestDirectory {
        path: std::path::PathBuf,
    }

    impl ProvisioningTestDirectory {
        fn new() -> Self {
            let root = std::path::PathBuf::from(
                std::env::var_os("BLUEY_TEST_RUN_ROOT")
                    .expect("canonical owned test root required"),
            );
            assert_eq!(
                std::fs::canonicalize(&root).expect("canonical test root"),
                root
            );
            let root_meta = std::fs::symlink_metadata(&root).unwrap();
            assert!(root_meta.is_dir() && !root_meta.file_type().is_symlink());
            let parent = std::fs::canonicalize(std::env::temp_dir()).expect("canonical test temp");
            assert!(parent.starts_with(&root) && parent != root);
            let parent_meta = std::fs::symlink_metadata(&parent).unwrap();
            assert!(parent_meta.is_dir() && !parent_meta.file_type().is_symlink());
            let marker_path = root.join(".bluey-test-run-v2");
            let marker_meta = std::fs::symlink_metadata(&marker_path).expect("owned run marker");
            assert!(marker_meta.is_file() && !marker_meta.file_type().is_symlink());
            assert!(marker_meta.len() <= 8192);
            #[cfg(unix)]
            let marker = {
                use std::os::unix::fs::MetadataExt;
                let id = std::process::Command::new("/usr/bin/id")
                    .arg("-u")
                    .env_clear()
                    .output()
                    .expect("current test UID");
                assert!(id.status.success());
                let uid: u32 = std::str::from_utf8(&id.stdout)
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                assert_eq!(root_meta.uid(), uid);
                assert_eq!(parent_meta.uid(), uid);
                assert_eq!(root_meta.mode() & 0o7777, 0o700);
                assert_eq!(parent_meta.mode() & 0o7777, 0o700);
                crate::db::owned_postgres_test::private_file(&marker_path, uid)
            };
            #[cfg(not(unix))]
            let marker = std::fs::read_to_string(&marker_path).expect("owned run marker text");
            let lines: Vec<_> = marker.lines().collect();
            assert_eq!(lines.len(), 9);
            assert_eq!(lines[0], "magic=bluey-test-run-v2");
            assert_eq!(lines[1], format!("run_root={}", root.display()));
            assert_eq!(
                lines[2],
                format!("run_name={}", root.file_name().unwrap().to_string_lossy())
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(lines[3], format!("owner_uid={}", root_meta.uid()));
            }
            assert_eq!(lines[8], "state=running");
            let name = format!(
                "bluey-provisioning-storage-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            );
            let path = parent.join(&name);
            std::fs::create_dir(&path)
                .expect("create isolated provisioning storage test directory");
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    // These fixtures exercise storage constraints, NOT real release/fleet/resource proof.
    const COLUMNS: &[&str] = &[
        "activation_sha256",
        "activation_id",
        "activation_generation",
        "trust_generation",
        "environment",
        "region",
        "channel",
        "channel_sequence",
        "expected_head_revision",
        "expected_transition_sha256",
        "predecessor_activation_sha256",
        "manifest_sha256",
        "manifest_signature_set_sha256",
        "cohort_sha256",
        "cohort_signature_set_sha256",
        "feature_authority_sha256",
        "cloud_distribution_enabled",
        "workflow_command_dispatch_enabled",
        "workflow_cleanup_enabled",
        "direct_discovery_enabled",
        "global_discovery_enabled",
        "source_verification_enabled",
        "runner_fleet_evidence_sha256",
        "cleanup_authority_sha256",
        "temporal_namespace_sha256",
        "storage_config_sha256",
        "task_queue_sha256",
        "failure_converter_sha256",
        "canary_evidence_sha256",
        "portal_readback_evidence_sha256",
        "portal_readback_at_ms",
        "portal_readback_ttl_ms",
        "maximum_inflight",
        "maximum_daily_admissions",
        "requirement_count",
        "heartbeat_ttl_ms",
        "recovery_acceptance_count",
        "canonical_activation_base64url",
        "authorization_signature_set_sha256",
        "issued_at_ms",
        "not_before_ms",
        "expires_at_ms",
        "recorded_by",
        "recorded_at_ms",
        "authority_kind",
        "purpose",
        "context_sha256",
        "queue_generation_id",
    ];

    // Explicit dependency order for transferring ONLY the normal unit fixture.
    const LEGACY_TABLES: &[&str] = &[
        "signature_sets",
        "signatures",
        "trust_policies",
        "trust_keys",
        "manifests",
        "manifest_components",
        "manifest_capabilities",
        "manifest_runtime_identities",
        "manifest_protocols",
        "manifest_source_verification_protocols",
        "manifest_original_source_verifier_identities",
        "manifest_source_verifier_startup_protocols",
        "manifest_source_verifier_boot_protocols",
        "cohorts",
        "cohort_members",
        "activations",
        "activation_requirements",
        "activation_recovery_acceptances",
        "head_transitions",
        "heads",
    ];

    struct UnitFixture {
        legacy_sha256: String,
        transition_sha256: String,
        activation: jobs::ManagedCloudActivationAuthority,
        now: i64,
    }

    fn quoted(value: &str) -> String {
        format!("'{}'", value.replace('\'', "''"))
    }

    fn execute(pool: &DbPool, sql: &str) -> Result<(), String> {
        match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .execute_batch(sql)
                .map_err(|error| error.to_string()),
            DbPool::Postgres(_) => pool.get_pg().unwrap().batch_execute(sql).map_err(|error| {
                error
                    .as_db_error()
                    .map(|error| error.message().to_owned())
                    .unwrap_or_else(|| error.to_string())
            }),
        }
    }

    fn text(pool: &DbPool, sql: &str) -> String {
        match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(sql, [], |row| row.get(0))
                .unwrap(),
            DbPool::Postgres(_) => pool.get_pg().unwrap().query_one(sql, &[]).unwrap().get(0),
        }
    }

    fn count(pool: &DbPool, sql: &str) -> i64 {
        match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(sql, [], |row| row.get(0))
                .unwrap(),
            DbPool::Postgres(_) => pool.get_pg().unwrap().query_one(sql, &[]).unwrap().get(0),
        }
    }

    fn copy_normal_legacy_fixture_to_pre067(
        source_path: &std::path::Path,
        target: &DbPool,
        include_heads: bool,
    ) {
        let mut conn = target.get().unwrap();
        conn.execute(
            "ATTACH DATABASE ?1 AS c1_source",
            [source_path.to_str().unwrap()],
        )
        .unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        let tx = conn.transaction().unwrap();
        for suffix in LEGACY_TABLES {
            if !include_heads && ["head_transitions", "heads"].contains(suffix) {
                continue;
            }
            let table = format!("jobs_managed_cloud_{suffix}");
            let columns = tx
                .prepare(&format!("PRAGMA main.table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert!(!columns.is_empty());
            let columns = columns.join(",");
            tx.execute_batch(&format!(
                "INSERT INTO main.{table}({columns}) SELECT {columns} FROM c1_source.{table};"
            ))
            .unwrap();
        }
        assert!(tx
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none());
        tx.commit().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch("DETACH DATABASE c1_source;").unwrap();
    }

    fn fixture(pool: &DbPool) -> UnitFixture {
        let now = count(
            pool,
            match pool {
                DbPool::Sqlite(_) => "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                DbPool::Postgres(_) => {
                    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint"
                }
            },
        );
        let scope = ManagedCloudScope {
            environment: "production".into(),
            region: "us-east-1".into(),
            channel: "general".into(),
        };
        let (legacy_sha256, _) = legacy_tests::install_valid_original_source_managed_head(
            pool,
            3,
            now,
            30_000,
            600_000,
            scope,
            "phase648-storage-unit-only",
        );
        let canonical = text(pool, &format!(
            "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations WHERE activation_sha256={}",
            quoted(&legacy_sha256),
        ));
        let activation =
            managed_cloud_parse_canonical(&managed_cloud_decode_base64url(&canonical).unwrap())
                .unwrap();
        let transition_sha256 = text(pool,
            "SELECT current_transition_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'");
        UnitFixture {
            legacy_sha256,
            transition_sha256,
            activation,
            now,
        }
    }

    fn document(unit: &UnitFixture) -> ProvisioningDocument {
        let legacy = &unit.activation;
        ProvisioningDocument {
            version: 4,
            audience: PROVISIONING_AUDIENCE.into(),
            kind: "provisioned_nonserving".into(),
            activation_id: "phase648-unit-provisioning".into(),
            activation_generation: legacy.activation_generation + 1,
            trust_generation: legacy.trust_generation,
            scope: legacy.scope.clone(),
            channel_sequence: legacy.channel_sequence + 1,
            expected_head_revision: 1,
            expected_transition_sha256: Some(unit.transition_sha256.clone()),
            predecessor_activation_sha256: Some(unit.legacy_sha256.clone()),
            manifest_sha256: legacy.manifest_sha256.clone(),
            manifest_signature_set_sha256: legacy.manifest_signature_set_sha256.clone(),
            cohort_sha256: legacy.cohort_sha256.clone(),
            cohort_signature_set_sha256: legacy.cohort_signature_set_sha256.clone(),
            intended_feature_authority_sha256: legacy.feature_authority_sha256.clone(),
            intended_feature_authority: legacy.feature_authority,
            temporal_namespace_sha256: legacy.temporal_namespace_sha256.clone(),
            storage_config_sha256: legacy.storage_config_sha256.clone(),
            task_queue_sha256: jobs::managed_cloud_task_queue_sha256(
                "phase648-unit",
                "phase648-storage-unit-queue",
            )
            .unwrap(),
            failure_converter_sha256: legacy.failure_converter_sha256.clone(),
            maximum_inflight: legacy.maximum_inflight,
            maximum_daily_admissions: legacy.maximum_daily_admissions,
            heartbeat_ttl_ms: legacy.heartbeat_ttl_ms,
            recovery_acceptances: Vec::new(),
            issued_at_ms: unit.now,
            not_before_ms: unit.now,
            expires_at_ms: unit.now + 120_000,
            authority_binding: AuthorityBinding {
                authority_kind: AuthorityKind::ProvisioningV4,
                purpose: Purpose::Production,
                context_sha256: None,
                queue_generation_id: "phase648-unit-queue-generation".into(),
            },
        }
    }

    fn insert_queue(pool: &DbPool, unit: &UnitFixture, doc: &ProvisioningDocument) {
        execute(pool, &format!(
            "INSERT INTO jobs_managed_cloud_queue_generations(queue_generation_id,issuance_ref,environment,region,channel,purpose,namespace,task_queue,task_queue_sha256,created_at_ms) VALUES({},'phase648-unit-queue-issuance','production','us-east-1','general','production','phase648-unit','phase648-storage-unit-queue',{},{});",
            quoted(&doc.authority_binding.queue_generation_id), quoted(&doc.task_queue_sha256), unit.now,
        )).unwrap();
    }

    fn signature(pool: &DbPool, doc: &ProvisioningDocument) -> (String, String, String) {
        let bytes = jobs::managed_cloud_canonical_json(doc).unwrap();
        let target = managed_cloud_sha256(&bytes);
        // Fixed synthetic key matches the existing normal unit trust-policy fixture.
        let key = ed25519_dalek::SigningKey::from_bytes(&[1; 32]);
        let mut set = jobs::ManagedCloudSignatureSetAuthority {
            version: 1,
            audience: jobs::MANAGED_CLOUD_SIGNATURE_SET_AUDIENCE.into(),
            signature_set_id: format!("phase648-unit-{}", &target[..16]),
            trust_generation: 1,
            role: "general_promotion".into(),
            target_audience: PROVISIONING_AUDIENCE.into(),
            target_sha256: target.clone(),
            signed_at_ms: doc.issued_at_ms,
            signatures: Vec::new(),
        };
        let payload = jobs::managed_cloud_signature_payload(&set).unwrap();
        set.signatures
            .push(jobs::ManagedCloudDetachedSignatureAuthority {
                key_id: "phase614b-general-promotion".into(),
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(key.sign(&payload).to_bytes()),
            });
        jobs::validate_managed_cloud_signature_set(&set).unwrap();
        let set_bytes = jobs::managed_cloud_canonical_json(&set).unwrap();
        let set_hash = managed_cloud_sha256(&set_bytes);
        let set_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(set_bytes);
        match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get().unwrap();
                let tx = conn.transaction().unwrap();
                jobs::insert_sqlite_managed_cloud_signature_set(
                    &tx,
                    &set,
                    &set_hash,
                    &set_b64,
                    "phase648-unit-only",
                    doc.issued_at_ms,
                )
                .unwrap();
                tx.commit().unwrap();
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().unwrap();
                let mut tx = conn.transaction().unwrap();
                jobs::insert_postgres_managed_cloud_signature_set(
                    &mut tx,
                    &set,
                    &set_hash,
                    &set_b64,
                    "phase648-unit-only",
                    doc.issued_at_ms,
                )
                .unwrap();
                tx.commit().unwrap();
            }
        }
        (
            target,
            set_hash,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
        )
    }

    fn candidate_sql(
        pool: &DbPool,
        unit: &UnitFixture,
        doc: &ProvisioningDocument,
        signed: &(String, String, String),
        changes: &[(&str, String)],
    ) -> String {
        let mut values = std::collections::BTreeMap::<&str, String>::new();
        for (key, value) in [
            ("activation_sha256", quoted(&signed.0)),
            ("activation_id", quoted(&doc.activation_id)),
            (
                "activation_generation",
                doc.activation_generation.to_string(),
            ),
            ("channel_sequence", doc.channel_sequence.to_string()),
            (
                "expected_head_revision",
                doc.expected_head_revision.to_string(),
            ),
            (
                "expected_transition_sha256",
                doc.expected_transition_sha256
                    .as_deref()
                    .map(quoted)
                    .unwrap_or_else(|| "NULL".into()),
            ),
            (
                "predecessor_activation_sha256",
                doc.predecessor_activation_sha256
                    .as_deref()
                    .map(quoted)
                    .unwrap_or_else(|| "NULL".into()),
            ),
            ("task_queue_sha256", quoted(&doc.task_queue_sha256)),
            ("canonical_activation_base64url", quoted(&signed.2)),
            ("authorization_signature_set_sha256", quoted(&signed.1)),
            ("issued_at_ms", doc.issued_at_ms.to_string()),
            ("not_before_ms", doc.not_before_ms.to_string()),
            ("expires_at_ms", doc.expires_at_ms.to_string()),
            ("recorded_at_ms", doc.issued_at_ms.to_string()),
            ("authority_kind", quoted("provisioning_v4")),
            ("purpose", quoted("production")),
            ("context_sha256", "NULL".into()),
            (
                "queue_generation_id",
                quoted(&doc.authority_binding.queue_generation_id),
            ),
        ] {
            values.insert(key, value);
        }
        for key in [
            "cloud_distribution_enabled",
            "workflow_command_dispatch_enabled",
            "workflow_cleanup_enabled",
            "direct_discovery_enabled",
            "global_discovery_enabled",
            "source_verification_enabled",
        ] {
            values.insert(
                key,
                match pool {
                    DbPool::Sqlite(_) => "0",
                    DbPool::Postgres(_) => "FALSE",
                }
                .into(),
            );
        }
        for key in [
            "runner_fleet_evidence_sha256",
            "cleanup_authority_sha256",
            "canary_evidence_sha256",
            "portal_readback_evidence_sha256",
            "portal_readback_at_ms",
            "portal_readback_ttl_ms",
        ] {
            values.insert(key, "NULL".into());
        }
        for (key, value) in changes {
            assert!(COLUMNS.contains(key));
            values.insert(*key, value.clone());
        }
        let projection = COLUMNS
            .iter()
            .map(|column| {
                values
                    .get(*column)
                    .cloned()
                    .unwrap_or_else(|| format!("legacy.{column}"))
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("INSERT INTO jobs_managed_cloud_activations({}) SELECT {projection} FROM jobs_managed_cloud_activations legacy WHERE legacy.activation_sha256={};",
            COLUMNS.join(","), quoted(&unit.legacy_sha256))
    }

    fn head_install_sql(
        unit: &UnitFixture,
        doc: &ProvisioningDocument,
        sha: &str,
        trust_generation: i64,
        channel_sequence: i64,
    ) -> String {
        let transition = managed_cloud_sha256(b"phase648-storage-unit-only-transition");
        format!(
            "INSERT INTO jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,previous_head_revision,previous_transition_sha256,previous_activation_sha256,previous_manifest_sha256,previous_trust_generation,previous_channel_sequence,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence,transition_kind,authority_sha256,rollback_authority_sha256,recorded_by,recorded_at_ms) SELECT {},environment,region,channel,2,1,current_transition_sha256,current_activation_sha256,current_manifest_sha256,current_trust_generation,current_channel_sequence,{},{},{},{},'provisioning',{},NULL,'phase648-unit-only',{} FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general';
             UPDATE jobs_managed_cloud_heads SET head_revision=2,current_transition_sha256={},current_activation_sha256={},current_manifest_sha256={},current_trust_generation={},current_channel_sequence={},updated_at_ms={} WHERE environment='production' AND region='us-east-1' AND channel='general';",
            quoted(&transition), quoted(sha), quoted(&doc.manifest_sha256), trust_generation,
            channel_sequence, quoted(sha), unit.now, quoted(&transition), quoted(sha),
            quoted(&doc.manifest_sha256), trust_generation, channel_sequence, unit.now,
        )
    }

    fn install_head(pool: &DbPool, unit: &UnitFixture, doc: &ProvisioningDocument, sha: &str) {
        execute(
            pool,
            &head_install_sql(unit, doc, sha, doc.trust_generation, doc.channel_sequence),
        )
        .unwrap();
    }

    /// Synthetic construction fixture ONLY. It proves no real resource ownership,
    /// runner startup, production canary, queue isolation or serving permission.
    pub(in crate::db::jobs) fn install_runtime_unit_fixture(
        pool: &DbPool,
    ) -> (jobs::NewManagedCloudRuntimeGrant, AuthorityBinding) {
        install_runtime_unit_fixture_from_unit(pool, fixture(pool))
    }

    /// Unit metadata only: seed the ordinary signed full-capability legacy head
    /// for the boot-aware original-source receipt grant regression.
    pub(in crate::db::jobs) fn seed_receipt_grant_legacy_head(pool: &DbPool) -> String {
        let now = count(
            pool,
            match pool {
                DbPool::Sqlite(_) => "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                DbPool::Postgres(_) => {
                    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint"
                }
            },
        );
        // The normal source/application fixture later requests a 15-minute
        // grant. Seed its REAL 24-hour legacy authority, not C1's 10-minute
        // schema-only authority (which would force a conflicting reinstall).
        let (sha, _) = legacy_tests::install_valid_original_source_managed_head(
            pool,
            3,
            now,
            300_000,
            24 * 60 * 60_000,
            ManagedCloudScope {
                environment: "production".into(),
                region: "us-east-1".into(),
                channel: "general".into(),
            },
            "phase648-receipt-grant-unit-only",
        );
        let canonical = text(pool, &format!(
            "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations WHERE activation_sha256={}", quoted(&sha),
        ));
        let activation: jobs::ManagedCloudActivationAuthority =
            managed_cloud_parse_canonical(&managed_cloud_decode_base64url(&canonical).unwrap())
                .unwrap();
        assert_eq!(
            managed_cloud_required_roles_for_feature(&activation.feature_authority).len(),
            9
        );
        sha
    }

    /// Reuse the exact immutable receipt fixture head and its signed trust policy.
    pub(in crate::db::jobs) fn install_receipt_grant_provisioning_head(
        pool: &DbPool,
        expected_legacy_sha256: &str,
    ) -> (jobs::NewManagedCloudRuntimeGrant, AuthorityBinding) {
        let legacy_sha256 = text(pool,
            "SELECT current_activation_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general' AND head_revision=1");
        assert_eq!(legacy_sha256, expected_legacy_sha256);
        let canonical = text(pool, &format!(
            "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations WHERE activation_sha256={}",
            quoted(&legacy_sha256),
        ));
        let activation: jobs::ManagedCloudActivationAuthority =
            managed_cloud_parse_canonical(&managed_cloud_decode_base64url(&canonical).unwrap())
                .unwrap();
        assert_eq!(
            managed_cloud_required_roles_for_feature(&activation.feature_authority).len(),
            9
        );
        let now = count(
            pool,
            match pool {
                DbPool::Sqlite(_) => "SELECT CAST(strftime('%s','now') AS INTEGER)*1000",
                DbPool::Postgres(_) => {
                    "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint"
                }
            },
        );
        let unit = UnitFixture {
            legacy_sha256,
            transition_sha256: text(pool,
                "SELECT current_transition_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general' AND head_revision=1"),
            activation,
            now,
        };
        install_runtime_unit_fixture_from_unit(pool, unit)
    }

    fn install_runtime_unit_fixture_from_unit(
        pool: &DbPool,
        unit: UnitFixture,
    ) -> (jobs::NewManagedCloudRuntimeGrant, AuthorityBinding) {
        let mut doc = document(&unit);
        doc.temporal_namespace_sha256 = managed_cloud_sha256(b"phase648-unit");
        insert_queue(pool, &unit, &doc);
        let signed = signature(pool, &doc);
        execute(
            pool,
            &candidate_sql(
                pool,
                &unit,
                &doc,
                &signed,
                &[(
                    "temporal_namespace_sha256",
                    quoted(&doc.temporal_namespace_sha256),
                )],
            ),
        )
        .unwrap();
        for (ordinal, role) in
            managed_cloud_required_roles_for_feature(&doc.intended_feature_authority)
                .into_iter()
                .enumerate()
        {
            let component = match role.as_str() {
                "managed_runner" => "jobs-runner",
                "jobs_api" | "workflow_command_dispatcher" | "workflow_cleanup_dispatcher" => {
                    "jobs-api"
                }
                _ => "jobs-workflows",
            };
            let artifact = text(pool, &format!("SELECT artifact_sha256 FROM jobs_managed_cloud_manifest_components WHERE manifest_sha256={} AND component_id={}",
                quoted(&doc.manifest_sha256), quoted(component)));
            let dependency =
                jobs::managed_cloud_provisioning_protocol::dependency_evidence_sha256_v2(
                    &jobs::managed_cloud_provisioning_protocol::RuntimeDependencyBinding {
                        role: &role,
                        activation_sha256: &signed.0,
                        manifest_sha256: &doc.manifest_sha256,
                        component_id: component,
                        artifact_sha256: &artifact,
                        task_queue_sha256: &doc.task_queue_sha256,
                        failure_converter_sha256: &doc.failure_converter_sha256,
                    },
                    &doc.authority_binding,
                )
                .unwrap();
            execute(pool, &format!("INSERT INTO jobs_managed_cloud_activation_requirements(activation_sha256,role,minimum_ready_instances,heartbeat_ttl_ms,dependency_evidence_sha256,ordinal) VALUES({},{},1,{},{},{});",
                quoted(&signed.0), quoted(&role), doc.heartbeat_ttl_ms, quoted(&dependency), ordinal)).unwrap();
        }
        install_head(pool, &unit, &doc, &signed.0);
        (
            jobs::NewManagedCloudRuntimeGrant {
                issuance_ref: "phase648-runtime-unit-issuance".into(),
                scope: doc.scope,
                activation_sha256: signed.0,
                manifest_sha256: doc.manifest_sha256,
                component_id: "jobs-api".into(),
                role: "jobs_api".into(),
                expected_worker_id: "phase648-runtime-unit-worker".into(),
                authorization_ref: "phase648-runtime-unit-authorization".into(),
                created_by: "phase648-unit-only".into(),
                ttl_ms: 60_000,
            },
            doc.authority_binding,
        )
    }

    fn assert_relational_head_tuple_mismatches_refuse_readiness(
        pool: &DbPool,
        unit: &UnitFixture,
        doc: &ProvisioningDocument,
        sha: &str,
    ) {
        let query = jobs::ManagedCloudReadinessQuery {
            scope: doc.scope.clone(),
            account_id: None,
        };
        for (trust, sequence) in [
            (doc.trust_generation + 1, doc.channel_sequence),
            (doc.trust_generation, doc.channel_sequence + 1),
        ] {
            let sql = head_install_sql(unit, doc, sha, trust, sequence);
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get().unwrap();
                    let tx = conn.transaction().unwrap();
                    tx.execute_batch(&sql)
                        .expect("existing immediate FKs permit this divergent tuple");
                    let actual: (i64, i64) = tx.query_row(
                        "SELECT current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'",
                        [], |row| Ok((row.get(0)?, row.get(1)?)),
                    ).unwrap();
                    assert_eq!(actual, (trust, sequence));
                    assert!(matches!(
                        jobs::resolve_sqlite_managed_cloud_readiness_tx(&tx, &query, unit.now),
                        Err(ManagedCloudRegistryError::InvalidAuthority)
                    ));
                    tx.rollback().unwrap();
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg().unwrap();
                    let mut tx = conn.transaction().unwrap();
                    tx.batch_execute(&sql)
                        .expect("existing immediate FKs permit this divergent tuple");
                    let actual = tx.query_one(
                        "SELECT current_trust_generation,current_channel_sequence FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'", &[],
                    ).unwrap();
                    assert_eq!(
                        (actual.get::<_, i64>(0), actual.get::<_, i64>(1)),
                        (trust, sequence)
                    );
                    assert!(matches!(
                        jobs::resolve_postgres_managed_cloud_readiness_tx(
                            &mut tx, &query, unit.now
                        ),
                        Err(ManagedCloudRegistryError::InvalidAuthority)
                    ));
                    tx.rollback().unwrap();
                }
            }
            assert_eq!(
                count(
                    pool,
                    "SELECT COUNT(*) FROM jobs_managed_cloud_head_transitions"
                ),
                1
            );
            assert_eq!(text(pool, "SELECT current_activation_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'"), unit.legacy_sha256);
        }
    }

    fn assert_canonical_relational_projection(
        doc: &ProvisioningDocument,
        signed: &(String, String, String),
    ) {
        let head = StoredHeadMetadata {
            scope: doc.scope.clone(),
            head_revision: doc.expected_head_revision + 1,
            transition_sha256: managed_cloud_sha256(b"unit-projection-current-transition"),
            activation_sha256: signed.0.clone(),
            manifest_sha256: doc.manifest_sha256.clone(),
            cohort_sha256: doc.cohort_sha256.clone(),
            trust_generation: doc.trust_generation,
            channel_sequence: doc.channel_sequence,
            release_id: "unit-only-release".into(),
            release_sequence: 1,
            task_queue_sha256: doc.task_queue_sha256.clone(),
            failure_converter_sha256: doc.failure_converter_sha256.clone(),
            activation_expires_at_ms: doc.expires_at_ms,
            features: ManagedCloudFeatureAuthority {
                cloud_distribution: false,
                workflow_command_dispatch: false,
                workflow_cleanup: false,
                direct_discovery: false,
                global_discovery: false,
                source_verification: false,
            },
        };
        let columns = StoredAuthorityColumns {
            kind: "provisioning_v4".into(),
            canonical: Some(signed.2.clone()),
            purpose: "production".into(),
            context_sha256: None,
            queue_generation_id: Some(doc.authority_binding.queue_generation_id.clone()),
            feature_authority_sha256: doc.intended_feature_authority_sha256.clone(),
            activation_id: doc.activation_id.clone(),
            activation_generation: doc.activation_generation,
            manifest_signature_set_sha256: doc.manifest_signature_set_sha256.clone(),
            cohort_signature_set_sha256: doc.cohort_signature_set_sha256.clone(),
            temporal_namespace_sha256: doc.temporal_namespace_sha256.clone(),
            storage_config_sha256: doc.storage_config_sha256.clone(),
            maximum_inflight: doc.maximum_inflight,
            maximum_daily_admissions: doc.maximum_daily_admissions,
            heartbeat_ttl_ms: doc.heartbeat_ttl_ms,
            issued_at_ms: doc.issued_at_ms,
            not_before_ms: doc.not_before_ms,
            recovery_acceptance_count: 0,
            requirement_count: 9,
            expected_head_revision: doc.expected_head_revision,
            expected_transition_sha256: doc.expected_transition_sha256.clone(),
            predecessor_activation_sha256: doc.predecessor_activation_sha256.clone(),
            previous_head_revision: doc.expected_head_revision,
            previous_transition_sha256: doc.expected_transition_sha256.clone(),
            previous_activation_sha256: doc.predecessor_activation_sha256.clone(),
            head_trust_generation: doc.trust_generation,
            head_channel_sequence: doc.channel_sequence,
        };
        assert!(project_stored_authority(&head, columns.clone()).is_ok());
        macro_rules! reject_column {
            ($field:ident, $value:expr) => {{
                let mut bad = columns.clone();
                bad.$field = $value;
                assert!(
                    project_stored_authority(&head, bad).is_err(),
                    stringify!($field)
                );
            }};
        }
        reject_column!(activation_id, "different-unit-activation".into());
        reject_column!(activation_generation, doc.activation_generation + 1);
        reject_column!(head_trust_generation, doc.trust_generation + 1);
        reject_column!(head_channel_sequence, doc.channel_sequence + 1);
        reject_column!(manifest_signature_set_sha256, "f".repeat(64));
        reject_column!(cohort_signature_set_sha256, "f".repeat(64));
        reject_column!(temporal_namespace_sha256, "f".repeat(64));
        reject_column!(storage_config_sha256, "f".repeat(64));
        reject_column!(maximum_inflight, doc.maximum_inflight + 1);
        reject_column!(maximum_daily_admissions, doc.maximum_daily_admissions + 1);
        reject_column!(heartbeat_ttl_ms, doc.heartbeat_ttl_ms + 1);
        reject_column!(issued_at_ms, doc.issued_at_ms + 1);
        reject_column!(not_before_ms, doc.not_before_ms + 1);
        reject_column!(recovery_acceptance_count, 1);
        reject_column!(requirement_count, 8);
        reject_column!(expected_head_revision, doc.expected_head_revision + 1);
        reject_column!(previous_head_revision, doc.expected_head_revision + 1);
        reject_column!(expected_transition_sha256, Some("f".repeat(64)));
        reject_column!(previous_transition_sha256, Some("f".repeat(64)));
        reject_column!(predecessor_activation_sha256, Some("f".repeat(64)));
        reject_column!(previous_activation_sha256, Some("f".repeat(64)));
        reject_column!(feature_authority_sha256, "f".repeat(64));
        reject_column!(purpose, "nonproduction_synthetic".into());
        reject_column!(context_sha256, Some("f".repeat(64)));
        reject_column!(
            queue_generation_id,
            Some("different-queue-generation".into())
        );
        reject_column!(canonical, None);
        let mut wrong_head = head.clone();
        wrong_head.features.cloud_distribution = true;
        assert!(project_stored_authority(&wrong_head, columns.clone()).is_err());
        let mut wrong_head = head.clone();
        wrong_head.activation_expires_at_ms += 1;
        assert!(project_stored_authority(&wrong_head, columns.clone()).is_err());
        let mut wrong_head = head;
        wrong_head.task_queue_sha256 = "f".repeat(64);
        assert!(project_stored_authority(&wrong_head, columns).is_err());
    }

    fn historical_rollback(
        pool: &DbPool,
        unit: &UnitFixture,
    ) -> (
        jobs::ManagedCloudRollbackImportRequest,
        jobs::ManagedCloudRollbackAuthority,
    ) {
        // Normal APIs produce this immutable legacy history before the v4 head.
        // Synthetic test evidence is not real fleet/canary/runtime qualification.
        let release: jobs::ManagedCloudReleaseAuthority = managed_cloud_parse_canonical(
            &managed_cloud_decode_base64url(&text(
                pool,
                "SELECT canonical_manifest_base64url FROM jobs_managed_cloud_manifests",
            ))
            .unwrap(),
        )
        .unwrap();
        let mut successor = unit.activation.clone();
        successor.activation_id = "phase648-unit-legacy-successor".into();
        successor.activation_generation += 1;
        successor.channel_sequence += 1;
        successor.expected_head_revision = 1;
        successor.expected_transition_sha256 = Some(unit.transition_sha256.clone());
        successor.predecessor_activation_sha256 = Some(unit.legacy_sha256.clone());
        successor.issued_at_ms = unit.now;
        successor.not_before_ms = unit.now;
        successor.portal_readback_at_ms = unit.now;
        successor.portal_readback_ttl_ms = 660_000;
        let evidence = legacy_tests::managed_cloud_test_activation_evidence(
            &mut successor,
            &release,
            unit.now,
            unit.now + 660_000,
        );
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &successor,
            &successor.audience,
            "general_promotion",
            unit.now,
        );
        let imported = jobs::import_managed_cloud_activation(
            pool,
            &jobs::ManagedCloudActivationImportRequest {
                envelope: signed.envelope,
                evidence,
            },
            "phase648-unit-only",
        )
        .unwrap();
        let rollback = jobs::ManagedCloudRollbackAuthority {
            version: 1,
            audience: jobs::MANAGED_CLOUD_ROLLBACK_AUDIENCE.into(),
            rollback_id: "phase648-unit-historical-rollback".into(),
            rollback_generation: 1,
            trust_generation: 1,
            scope: unit.activation.scope.clone(),
            expected_head_revision: 1,
            expected_transition_sha256: unit.transition_sha256.clone(),
            from_activation_sha256: unit.legacy_sha256.clone(),
            from_manifest_sha256: unit.activation.manifest_sha256.clone(),
            to_activation_sha256: imported.authority_sha256,
            to_manifest_sha256: successor.manifest_sha256,
            evidence_sha256: managed_cloud_sha256(b"phase648-unit-only-rollback-evidence"),
            reason_ref: "phase648-unit-only-historical-replay".into(),
            issued_at_ms: unit.now,
        };
        let signed = legacy_tests::managed_cloud_test_signed_envelope(
            &rollback,
            &rollback.audience,
            "incident",
            unit.now,
        );
        let request = jobs::ManagedCloudRollbackImportRequest {
            envelope: signed.envelope,
        };
        assert!(
            !jobs::import_managed_cloud_rollback(pool, &request, "phase648-unit-only")
                .unwrap()
                .replayed
        );
        (request, rollback)
    }

    fn fresh_signed_rollback_with_different_migration_set_refuses_without_mutation(
        pool: &DbPool,
        unit: &UnitFixture,
        historical: &jobs::ManagedCloudRollbackAuthority,
    ) {
        // Test-only: the normal signed helper creates a distinct, self-consistent
        // same-version manifest. Only its immutable migration-set identity differs.
        let (release, release_request) =
            legacy_tests::managed_cloud_test_release_with_different_migration_set(
                pool,
                // The fixture helper signs one second after this timestamp.
                unit.now - 5_000,
                unit.activation.version,
            );
        let imported_release =
            jobs::import_managed_cloud_release(pool, &release_request, "phase648-unit-only")
                .expect("import signed same-version migration-drift release");
        let stored_migration_set = text(
            pool,
            "SELECT migration_set_sha256 FROM jobs_managed_cloud_manifests \
             WHERE manifest_sha256=(SELECT manifest_sha256 FROM jobs_managed_cloud_activations \
             WHERE activation_sha256=(SELECT current_activation_sha256 \
             FROM jobs_managed_cloud_heads WHERE environment='production' \
             AND region='us-east-1' AND channel='general'))",
        );
        assert_ne!(stored_migration_set, release.migration_set_sha256);

        let mut successor = unit.activation.clone();
        successor.activation_id = "phase648-unit-migration-set-mismatch".into();
        successor.activation_generation += 2;
        successor.channel_sequence += 2;
        successor.expected_head_revision = 1;
        successor.expected_transition_sha256 = Some(unit.transition_sha256.clone());
        successor.predecessor_activation_sha256 = Some(unit.legacy_sha256.clone());
        successor.manifest_sha256 = imported_release.authority_sha256.clone();
        successor.manifest_signature_set_sha256 = imported_release.signature_set_sha256;
        successor.feature_authority = release.feature_authority;
        successor.feature_authority_sha256 = release.feature_authority_sha256.clone();
        successor.issued_at_ms = unit.now;
        successor.not_before_ms = unit.now;
        successor.portal_readback_at_ms = unit.now;
        successor.portal_readback_ttl_ms = 660_000;
        let evidence = legacy_tests::managed_cloud_test_activation_evidence(
            &mut successor,
            &release,
            unit.now,
            unit.now + 660_000,
        );
        let signed_activation = legacy_tests::managed_cloud_test_signed_envelope(
            &successor,
            &successor.audience,
            "general_promotion",
            unit.now,
        );
        let imported_activation = jobs::import_managed_cloud_activation(
            pool,
            &jobs::ManagedCloudActivationImportRequest {
                envelope: signed_activation.envelope,
                evidence,
            },
            "phase648-unit-only",
        )
        .expect("import signed same-version migration-drift activation");

        let mut rollback = historical.clone();
        rollback.rollback_id = "phase648-unit-migration-set-mismatch".into();
        rollback.rollback_generation += 1;
        rollback.to_activation_sha256 = imported_activation.authority_sha256;
        rollback.to_manifest_sha256 = successor.manifest_sha256.clone();
        rollback.reason_ref = "phase648-unit-migration-set-mismatch".into();
        let signed_rollback = legacy_tests::managed_cloud_test_signed_envelope(
            &rollback,
            &rollback.audience,
            "incident",
            unit.now,
        );
        let before = (
            count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_manifests"),
            count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations"),
            count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_rollbacks"),
            count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_heads"),
            count(
                pool,
                "SELECT COUNT(*) FROM jobs_managed_cloud_head_transitions",
            ),
            text(
                pool,
                "SELECT current_transition_sha256 || ':' || current_activation_sha256 || ':' || \
                 current_manifest_sha256 || ':' || current_trust_generation || ':' || \
                 current_channel_sequence FROM jobs_managed_cloud_heads \
                 WHERE environment='production' AND region='us-east-1' AND channel='general'",
            ),
        );
        assert!(matches!(
            jobs::import_managed_cloud_rollback(
                pool,
                &jobs::ManagedCloudRollbackImportRequest {
                    envelope: signed_rollback.envelope,
                },
                "phase648-unit-only",
            ),
            Err(ManagedCloudRegistryError::InvalidAuthority)
        ));
        assert_eq!(
            (
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_manifests"),
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations"),
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_rollbacks"),
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_heads"),
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_head_transitions"),
                text(
                    pool,
                    "SELECT current_transition_sha256 || ':' || current_activation_sha256 || ':' || \
                     current_manifest_sha256 || ':' || current_trust_generation || ':' || \
                     current_channel_sequence FROM jobs_managed_cloud_heads \
                     WHERE environment='production' AND region='us-east-1' AND channel='general'",
                ),
            ),
            before,
        );
        println!("phase660_registry_migration_mismatch_refused");
    }

    fn assert_constraints(pool: &DbPool, unit: UnitFixture) {
        let (historical_request, historical) = historical_rollback(pool, &unit);
        fresh_signed_rollback_with_different_migration_set_refuses_without_mutation(
            pool,
            &unit,
            &historical,
        );
        let baseline = count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations");
        let mut doc = document(&unit);
        // Two legacy successors were imported without advancing the head.
        doc.activation_generation += 2;
        doc.channel_sequence += 2;
        insert_queue(pool, &unit, &doc);
        assert!(execute(pool, &format!("INSERT INTO jobs_managed_cloud_queue_generations(queue_generation_id,issuance_ref,environment,region,channel,purpose,namespace,task_queue,task_queue_sha256,created_at_ms) VALUES(NULL,'phase648-null-queue-issuance','production','us-east-1','general','production','c1-null-namespace','c1-null-queue',{},{});", quoted(&"f".repeat(64)), unit.now)).is_err());
        assert!(execute(pool, &format!("INSERT INTO jobs_managed_cloud_queue_generations(queue_generation_id,issuance_ref,environment,region,channel,purpose,namespace,task_queue,task_queue_sha256,created_at_ms) VALUES('phase648-reused-physical-queue','phase648-reused-queue-issuance','production','us-east-1','general','production','phase648-unit','phase648-storage-unit-queue',{},{});", quoted(&"e".repeat(64)), unit.now)).is_err());
        let signed = signature(pool, &doc);
        assert_canonical_relational_projection(&doc, &signed);
        let true_value = match pool {
            DbPool::Sqlite(_) => "1",
            DbPool::Postgres(_) => "TRUE",
        };
        let expires_now = match pool {
            DbPool::Sqlite(_) => "CAST(strftime('%s','now') AS INTEGER)*1000+CAST(substr(strftime('%f','now'),4,3) AS INTEGER)",
            DbPool::Postgres(_) => "floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
        };
        for changes in [
            vec![("cloud_distribution_enabled", true_value.into())],
            vec![("runner_fleet_evidence_sha256", quoted(&"1".repeat(64)))],
            vec![("canonical_activation_base64url", "NULL".into())],
            vec![("queue_generation_id", "NULL".into())],
            vec![("requirement_count", "8".into())],
            vec![("purpose", quoted("nonproduction_synthetic"))],
            vec![("expires_at_ms", (unit.now - 1).to_string())],
            vec![("expires_at_ms", expires_now.into())],
            vec![("feature_authority_sha256", quoted(&"0".repeat(64)))],
            vec![
                ("authority_kind", quoted("legacy_activation")),
                ("queue_generation_id", "NULL".into()),
            ],
        ] {
            assert!(execute(pool, &candidate_sql(pool, &unit, &doc, &signed, &changes)).is_err());
            assert_eq!(
                count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations"),
                baseline
            );
        }
        execute(pool, &candidate_sql(pool, &unit, &doc, &signed, &[])).unwrap();
        assert_eq!(count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations WHERE authority_kind='provisioning_v4'"), 1);
        let mut duplicate = doc.clone();
        duplicate.activation_id = "phase648-unit-second-activation".into();
        duplicate.activation_generation += 1;
        duplicate.channel_sequence += 1;
        let second = signature(pool, &duplicate);
        assert!(execute(pool, &candidate_sql(pool, &unit, &duplicate, &second, &[])).is_err());
        assert!(execute(pool, "UPDATE jobs_managed_cloud_queue_generations SET issuance_ref='phase648-modified-issuance'").is_err());
        assert!(execute(pool, "DELETE FROM jobs_managed_cloud_queue_generations").is_err());
        assert_relational_head_tuple_mismatches_refuse_readiness(pool, &unit, &doc, &signed.0);
        install_head(pool, &unit, &doc, &signed.0);
        let status = jobs::get_managed_cloud_release_status(pool, &doc.scope).unwrap();
        assert_eq!(status.activation_sha256.as_deref(), Some(signed.0.as_str()));
        assert_eq!(status.head_revision, 2);
        assert_eq!(
            status.unavailability_reason.as_deref(),
            Some("provisioned_nonserving")
        );
        assert!(!status.authority_ready && !status.customer_admission);
        let query = jobs::ManagedCloudReadinessQuery {
            scope: doc.scope.clone(),
            account_id: None,
        };
        let public_readiness = jobs::resolve_managed_cloud_readiness(pool, &query).unwrap();
        let transactional_readiness = match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get().unwrap();
                let tx = conn.transaction().unwrap();
                let result =
                    jobs::resolve_sqlite_managed_cloud_readiness_tx(&tx, &query, unit.now).unwrap();
                tx.rollback().unwrap();
                result
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().unwrap();
                let mut tx = conn.transaction().unwrap();
                let result =
                    jobs::resolve_postgres_managed_cloud_readiness_tx(&mut tx, &query, unit.now)
                        .unwrap();
                tx.rollback().unwrap();
                result
            }
        };
        assert_eq!(public_readiness.status, status);
        assert_eq!(transactional_readiness.status, status);
        assert!(!public_readiness.cohort_eligible && !transactional_readiness.cohort_eligible);
        assert!(
            jobs::import_managed_cloud_rollback(pool, &historical_request, "phase648-unit-only")
                .unwrap()
                .replayed
        );
        let mut conflicting = historical.clone();
        conflicting.reason_ref = "phase648-unit-conflicting-replay".into();
        let signed_conflict = legacy_tests::managed_cloud_test_signed_envelope(
            &conflicting,
            &conflicting.audience,
            "incident",
            unit.now,
        );
        assert!(matches!(
            jobs::import_managed_cloud_rollback(
                pool,
                &jobs::ManagedCloudRollbackImportRequest {
                    envelope: signed_conflict.envelope,
                },
                "phase648-unit-only"
            ),
            Err(ManagedCloudRegistryError::IdentityConflict)
        ));
        let mut fresh = historical.clone();
        fresh.rollback_id = "phase648-unit-new-rollback".into();
        fresh.rollback_generation += 1;
        let signed_fresh = legacy_tests::managed_cloud_test_signed_envelope(
            &fresh,
            &fresh.audience,
            "incident",
            unit.now,
        );
        assert!(matches!(
            jobs::import_managed_cloud_rollback(
                pool,
                &jobs::ManagedCloudRollbackImportRequest {
                    envelope: signed_fresh.envelope,
                },
                "phase648-unit-only"
            ),
            Err(ManagedCloudRegistryError::Unavailable)
        ));
        assert!(jobs::apply_managed_cloud_rollback(
            pool,
            &jobs::ApplyManagedCloudRollbackRequest {
                rollback_sha256: managed_cloud_sha256(
                    &jobs::managed_cloud_canonical_json(&historical).unwrap()
                ),
                expected_head_revision: historical.expected_head_revision,
                expected_transition_sha256: historical.expected_transition_sha256.clone(),
            },
            "phase648-unit-only"
        )
        .is_err());
        assert_eq!(
            count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_rollbacks"),
            1
        );
        let downgrade = execute(pool, &format!("UPDATE jobs_managed_cloud_heads SET head_revision=3,current_transition_sha256={},current_activation_sha256={},current_manifest_sha256={},current_trust_generation={},current_channel_sequence={},updated_at_ms={} WHERE environment='production' AND region='us-east-1' AND channel='general';",
            quoted(&"d".repeat(64)), quoted(&unit.legacy_sha256), quoted(&unit.activation.manifest_sha256),
            unit.activation.trust_generation, unit.activation.channel_sequence, unit.now)).unwrap_err();
        assert!(downgrade.contains("cannot downgrade"));
        assert!(jobs::resolve_managed_cloud_admission(
            pool,
            &jobs::ManagedCloudReadinessQuery {
                scope: doc.scope.clone(),
                account_id: None,
            }
        )
        .is_err());
        let grant = jobs::NewManagedCloudRuntimeGrant {
            issuance_ref: "phase648-legacy-grant-issuance".into(),
            scope: doc.scope.clone(),
            activation_sha256: signed.0.clone(),
            manifest_sha256: doc.manifest_sha256.clone(),
            component_id: "jobs-runner".into(),
            role: "managed_runner".into(),
            expected_worker_id: "phase648-legacy-worker-identity".into(),
            authorization_ref: "phase648-legacy-authorization".into(),
            created_by: "phase648-unit-only".into(),
            ttl_ms: 10_000,
        };
        assert!(jobs::issue_managed_cloud_runtime_grant(pool, &grant).is_err());
        assert_eq!(
            count(
                pool,
                "SELECT COUNT(*) FROM jobs_managed_cloud_runtime_grants"
            ),
            0
        );
        let before = text(pool, "SELECT current_activation_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'");
        assert!(jobs::apply_managed_cloud_activation(
            pool,
            &jobs::ApplyManagedCloudActivationRequest {
                activation_sha256: unit.legacy_sha256.clone(),
                expected_head_revision: 0,
                expected_transition_sha256: None,
            },
            "phase648-unit-only"
        )
        .is_err());
        assert_eq!(text(pool, "SELECT current_activation_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'"), before);
        db::run_migrations(pool).unwrap();
        assert_eq!(text(pool, "SELECT current_activation_sha256 FROM jobs_managed_cloud_heads WHERE environment='production' AND region='us-east-1' AND channel='general'"), before);
        assert_eq!(count(pool, "SELECT COUNT(*) FROM jobs_managed_cloud_activations WHERE authority_kind='provisioning_v4'"), 1);
    }

    const PRE068_EXTERNAL_TRIGGERS: &[(&str, &str)] = &[
        (
            "trg_jobs_managed_cloud_execution_lease_insert",
            "jobs_execution_leases",
        ),
        (
            "trg_jobs_managed_cloud_execution_lease_update",
            "jobs_execution_leases",
        ),
        (
            "trg_jobs_original_source_receipt_binding_validate",
            "jobs_original_source_verification_receipts",
        ),
        (
            "trg_jobs_osv_boot_heartbeat_insert",
            "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
        ),
        (
            "trg_jobs_osv_boot_heartbeat_update",
            "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
        ),
    ];
    const PRE068_RUNTIME_TABLES: &[&str] = &[
        "jobs_managed_cloud_runtime_grants",
        "jobs_managed_cloud_runtime_grant_revocations",
        "jobs_managed_cloud_runtime_instances",
        "jobs_managed_cloud_runtime_heartbeats",
        "jobs_managed_cloud_runtime_heartbeat_audit",
        "jobs_managed_cloud_original_source_verifier_runtime_grants",
        "jobs_managed_cloud_original_source_verifier_grant_revocations",
        "jobs_managed_cloud_original_source_verifier_runtime_instances",
        "jobs_managed_cloud_original_source_verifier_runtime_heartbeats",
        "jobs_managed_cloud_original_source_verifier_runtime_heartbeat_audit",
        "jobs_managed_cloud_source_verifier_boot_generation_heads",
        "jobs_managed_cloud_source_verifier_boot_registrations",
        "jobs_managed_cloud_source_verifier_boot_startup_authorities",
        "jobs_managed_cloud_source_verifier_heartbeat_boot_bindings",
        "jobs_execution_leases",
        "jobs_original_source_verification_receipts",
    ];

    #[derive(Debug, PartialEq, Eq)]
    struct LegacyRowSnapshot {
        table: String,
        columns: Vec<String>,
        row_hashes: Vec<String>,
    }

    fn legacy_rows(
        conn: &rusqlite::Connection,
        table: &str,
        columns: Option<&[String]>,
    ) -> LegacyRowSnapshot {
        let columns = columns.map(<[String]>::to_vec).unwrap_or_else(|| {
            conn.prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        });
        assert!(!columns.is_empty());
        let mut statement = conn
            .prepare(&format!("SELECT {} FROM {table}", columns.join(",")))
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        let mut hashes = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let cells = (0..columns.len())
                .map(|index| {
                    use rusqlite::types::ValueRef;
                    match row.get_ref(index).unwrap() {
                        ValueRef::Null => (0, String::new()),
                        ValueRef::Integer(v) => (1, v.to_string()),
                        ValueRef::Real(v) => (2, v.to_bits().to_string()),
                        ValueRef::Text(v) => (
                            3,
                            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v),
                        ),
                        ValueRef::Blob(v) => (
                            4,
                            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v),
                        ),
                    }
                })
                .collect::<Vec<_>>();
            hashes.push(managed_cloud_sha256(&serde_json::to_vec(&cells).unwrap()));
        }
        hashes.sort();
        LegacyRowSnapshot {
            table: table.to_owned(),
            columns,
            row_hashes: hashes,
        }
    }

    fn pre068_rows(conn: &rusqlite::Connection) -> Vec<LegacyRowSnapshot> {
        LEGACY_TABLES
            .iter()
            .map(|suffix| format!("jobs_managed_cloud_{suffix}"))
            .chain(
                PRE068_RUNTIME_TABLES
                    .iter()
                    .map(|table| (*table).to_owned()),
            )
            .map(|table| legacy_rows(conn, &table, None))
            .collect()
    }
    fn assert_legacy_rows(conn: &rusqlite::Connection, before: &[LegacyRowSnapshot]) {
        for row in before {
            assert_eq!(legacy_rows(conn, &row.table, Some(&row.columns)), *row);
        }
    }
    fn sqlite_schema_snapshot(
        conn: &rusqlite::Connection,
    ) -> Vec<(String, String, String, Option<String>)> {
        conn.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }
    fn external_trigger_snapshot(conn: &rusqlite::Connection) -> Vec<(String, String, String)> {
        PRE068_EXTERNAL_TRIGGERS.iter().map(|(name, table)| {
            let row = conn.query_row("SELECT name,tbl_name,sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
                [name], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))).unwrap();
            assert_eq!(&row.1, table); row
        }).collect()
    }
    fn compiled_trigger_statement(name: &str) -> String {
        let migration = db::SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS;
        let marker = format!("CREATE TRIGGER {name}");
        let offsets = migration
            .match_indices(&marker)
            .filter(|(start, _)| {
                migration
                    .as_bytes()
                    .get(start + marker.len())
                    .is_some_and(u8::is_ascii_whitespace)
            })
            .map(|(start, _)| start)
            .collect::<Vec<_>>();
        assert_eq!(
            offsets.len(),
            1,
            "external trigger must have exactly one reviewed full definition"
        );
        let tail = &migration[offsets[0]..];
        let mut length = 0;
        for line in tail.split_inclusive('\n') {
            length += line.len();
            let statement = line.trim();
            if statement == "END;"
                || (statement.starts_with("BEGIN ") && statement.ends_with(" END;"))
            {
                return tail[..length].trim_end().trim_end_matches(';').to_owned();
            }
        }
        panic!("external trigger full statement is missing");
    }
    fn assert_connection_integrity(conn: &rusqlite::Connection) {
        assert!(conn.is_autocommit());
        assert!(conn
            .pragma_query_value::<bool, _>(None, "foreign_keys", |row| row.get(0))
            .unwrap());
        assert!(conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none());
        assert_eq!(
            conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    fn pre068_key_closure(
        conn: &rusqlite::Connection,
        tables: &[LegacyRowSnapshot],
    ) -> Vec<(String, Vec<String>, Vec<String>)> {
        tables
            .iter()
            .map(|table| {
                let mut grouped = std::collections::BTreeMap::<
                    i64,
                    Vec<(i64, String, String, String, String, String, String)>,
                >::new();
                for row in conn
                    .prepare(&format!("PRAGMA foreign_key_list({})", table.table))
                    .unwrap()
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            (
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                                row.get(7)?,
                            ),
                        ))
                    })
                    .unwrap()
                {
                    let (id, value) = row.unwrap();
                    grouped.entry(id).or_default().push(value);
                }
                let mut foreign = grouped
                    .into_values()
                    .map(|mut rows| {
                        rows.sort();
                        serde_json::to_string(&rows).unwrap()
                    })
                    .collect::<Vec<_>>();
                foreign.sort();
                let indexes = conn
                    .prepare(&format!("PRAGMA index_list({})", table.table))
                    .unwrap()
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                let mut keys = indexes
                    .into_iter()
                    .map(|(name, unique, origin, partial)| {
                        let columns = conn
                            .prepare(&format!("PRAGMA index_xinfo({name})"))
                            .unwrap()
                            .query_map([], |row| {
                                Ok((
                                    row.get::<_, i64>(0)?,
                                    row.get::<_, i64>(1)?,
                                    row.get::<_, Option<String>>(2)?,
                                    row.get::<_, i64>(3)?,
                                    row.get::<_, String>(4)?,
                                    row.get::<_, i64>(5)?,
                                ))
                            })
                            .unwrap()
                            .collect::<rusqlite::Result<Vec<_>>>()
                            .unwrap();
                        serde_json::to_string(&(
                            if origin == "c" { name } else { origin },
                            unique,
                            partial,
                            columns,
                        ))
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                keys.sort();
                (table.table.clone(), foreign, keys)
            })
            .collect()
    }

    fn pre068_instance(
        pool: &DbPool,
        unit: &UnitFixture,
        role: &str,
    ) -> (jobs::ManagedCloudRuntimeInstance, String) {
        let grant = jobs::issue_managed_cloud_runtime_grant(
            pool,
            &jobs::NewManagedCloudRuntimeGrant {
                issuance_ref: format!("phase648-pre068-unit-{role}"),
                scope: unit.activation.scope.clone(),
                activation_sha256: unit.legacy_sha256.clone(),
                manifest_sha256: unit.activation.manifest_sha256.clone(),
                component_id: if role == "jobs_api" {
                    "jobs-api"
                } else {
                    "jobs-workflows"
                }
                .into(),
                role: role.into(),
                expected_worker_id: format!("phase648-pre068-unit-worker-{role}"),
                authorization_ref: "phase648-pre068-unit-authorization".into(),
                created_by: "phase648-unit-only".into(),
                ttl_ms: 300_000,
            },
        )
        .unwrap();
        let token = grant.grant_token.unwrap();
        let runtime_id = format!("phase648-pre068-unit-runtime-{role}");
        let session =
            jobs::derive_managed_cloud_runtime_session_token(&token, &grant.grant_id, &runtime_id)
                .unwrap();
        let instance = jobs::claim_managed_cloud_runtime_grant(
            pool,
            &jobs::ClaimManagedCloudRuntimeGrant {
                grant_id: grant.grant_id,
                grant_token: token,
                runtime_instance_id: runtime_id,
                worker_id: grant.expected_worker_id,
                session_token: session.clone(),
                runtime_identity_sha256: grant.expected_runtime_identity_sha256,
            },
        )
        .unwrap();
        (instance, session)
    }
    fn pre068_heartbeat(
        instance: &jobs::ManagedCloudRuntimeInstance,
        session: &str,
        boot: Option<&jobs::SourceVerifierBootRegistration>,
    ) -> jobs::ManagedCloudRuntimeHeartbeatInput {
        jobs::ManagedCloudRuntimeHeartbeatInput {
            runtime_instance_id: instance.runtime_instance_id.clone(),
            worker_id: instance.worker_id.clone(),
            session_token: session.into(),
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
            boot_id: boot.map(|v| v.boot_id.clone()),
            boot_generation: boot.map(|v| v.boot_generation),
        }
    }

    #[test]
    fn c3_sqlite_populated_pre068_upgrade_preserves_runtime_rows_and_external_guards() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let directory = ProvisioningTestDirectory::new();
        let pool = db::open_pool(&directory.path().join("pre068.sqlite3")).unwrap();
        let position = db::MIGRATIONS
            .iter()
            .position(|migration| *migration == db::SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS)
            .expect("exact 068 must be registered; never assume last");
        assert_eq!(
            db::MIGRATIONS
                .iter()
                .filter(|migration| **migration == db::SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS)
                .count(),
            1
        );
        db::run_sqlite_migration_list(&pool, &db::MIGRATIONS[..position]).unwrap();
        let unit = fixture(&pool);
        let (base, base_session) = pre068_instance(&pool, &unit, "jobs_api");
        jobs::record_managed_cloud_runtime_heartbeat(
            &pool,
            &pre068_heartbeat(&base, &base_session, None),
        )
        .unwrap();
        let (osv, session) = pre068_instance(&pool, &unit, "original_source_verifier");
        let registration = jobs::register_source_verifier_boot(
            &pool,
            &jobs::SourceVerifierBootRegistrationRequest {
                session_token: session.clone(),
                registration_request_id: "phase648-pre068-unit-registration".into(),
                boot_id: "phase648-pre068-unit-boot".into(),
                runtime_instance_id: osv.runtime_instance_id.clone(),
                instance_epoch: osv.instance_epoch,
                worker_id: osv.worker_id.clone(),
                runtime_identity_sha256: osv.runtime_identity_sha256.clone(),
                activation_sha256: osv.activation_sha256.clone(),
                manifest_sha256: osv.manifest_sha256.clone(),
                component_id: osv.component_id.clone(),
                role: osv.role.clone(),
            },
        )
        .unwrap();
        jobs::record_managed_cloud_runtime_heartbeat(
            &pool,
            &pre068_heartbeat(&osv, &session, Some(&registration)),
        )
        .unwrap();
        // Current legacy startup's leaseability reader requires protocol columns
        // installed by 068. Do not fabricate pre-upgrade Ready or add columns.
        // A preexisting populated startup-row upgrade fixture remains a separate gap.
        let before;
        {
            let mut conn = pool.get().unwrap();
            assert_connection_integrity(&conn);
            before = pre068_rows(&conn);
            assert_eq!(
                before
                    .iter()
                    .find(|v| v.table
                        == "jobs_managed_cloud_source_verifier_boot_startup_authorities")
                    .unwrap()
                    .row_hashes
                    .len(),
                0
            );
            let schema = sqlite_schema_snapshot(&conn);
            let guards = external_trigger_snapshot(&conn);
            for anchor in [
                "ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD COLUMN protocol_version",
                "ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD COLUMN protocol_version",
                "ALTER TABLE jobs_managed_cloud_runtime_heartbeats ADD COLUMN observation_sha256",
                "ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_heartbeats ADD COLUMN observation_sha256",
                "DROP TABLE jobs_managed_cloud_runtime_instances;",
                "ALTER TABLE jobs_managed_cloud_runtime_instances_c3 RENAME TO jobs_managed_cloud_runtime_instances;",
                "CREATE TRIGGER trg_jobs_managed_cloud_execution_lease_insert",
                "DROP TABLE jobs_managed_cloud_original_source_verifier_runtime_instances;",
                "ALTER TABLE jobs_managed_cloud_original_source_verifier_runtime_instances_c3 RENAME TO jobs_managed_cloud_original_source_verifier_runtime_instances;",
                "CREATE TRIGGER trg_jobs_original_source_receipt_binding_validate",
                "CREATE TRIGGER trg_jobs_osv_boot_heartbeat_insert",
                "CREATE TRIGGER trg_jobs_osv_boot_heartbeat_update",
                "DROP TABLE jobs_managed_cloud_source_verifier_boot_registrations;",
                "ALTER TABLE jobs_managed_cloud_source_verifier_boot_registrations_c3 RENAME TO jobs_managed_cloud_source_verifier_boot_registrations;",
                "DROP TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities;",
                "ALTER TABLE jobs_managed_cloud_source_verifier_boot_startup_authorities_c3 RENAME TO jobs_managed_cloud_source_verifier_boot_startup_authorities;",
                "INSERT OR IGNORE INTO bluey_data_migrations(name) VALUES('jobs-provisioned-runtime-observations-storage-v1');",
            ] {
                let sql = db::SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS;
                assert_eq!(sql.matches(anchor).count(), 1, "injection must match exactly once");
                let interrupted = sql.replacen(anchor, &format!("SELECT * FROM c3_pre068_injected_failure;\n{anchor}"), 1);
                assert_ne!(interrupted, sql);
                let error = db::execute_sqlite_provisioned_runtime_observations_migration(&mut conn, &interrupted).unwrap_err();
                assert!(format!("{error:#}").contains("c3_pre068_injected_failure"), "must reach intended failure, not an earlier dangling trigger");
                assert_connection_integrity(&conn);
                assert_eq!(sqlite_schema_snapshot(&conn), schema);
                assert_eq!(external_trigger_snapshot(&conn), guards);
                assert_legacy_rows(&conn, &before);
                assert_eq!(conn.query_row("SELECT COUNT(*) FROM bluey_data_migrations WHERE name='jobs-provisioned-runtime-observations-storage-v1'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
            }
            let keys = pre068_key_closure(&conn, &before);
            db::execute_sqlite_provisioned_runtime_observations_migration(
                &mut conn,
                db::SQLITE_JOBS_PROVISIONED_RUNTIME_OBSERVATIONS,
            )
            .unwrap();
            assert_connection_integrity(&conn);
            assert_legacy_rows(&conn, &before);
            let after_keys = pre068_key_closure(&conn, &before);
            for ((table, foreign, indexes), (after_table, after_foreign, after_indexes)) in
                keys.iter().zip(&after_keys)
            {
                assert_eq!(table, after_table);
                assert!(
                    foreign.iter().all(|value| after_foreign.contains(value)),
                    "legacy composite FK changed"
                );
                assert!(
                    indexes.iter().all(|value| after_indexes.contains(value)),
                    "legacy index/unique key changed"
                );
            }
            for (name, _, sql) in external_trigger_snapshot(&conn) {
                assert_eq!(sql, compiled_trigger_statement(&name));
            }
            for table in [
                "jobs_managed_cloud_runtime_instances",
                "jobs_managed_cloud_original_source_verifier_runtime_instances",
            ] {
                assert_eq!(conn.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE protocol_version=1 AND claimed_latch_sha256 IS NULL"), [], |row| row.get::<_, i64>(0)).unwrap(), 1);
            }
        }
        let request = jobs::SourceVerifierBootStartupRequest {
            session_token: session,
            startup_request_id: "phase648-post068-unit-startup".into(),
            boot_id: registration.boot_id,
            runtime_instance_id: osv.runtime_instance_id,
            instance_epoch: osv.instance_epoch,
            boot_generation: registration.boot_generation,
            worker_id: osv.worker_id,
            degraded_heartbeat_sequence: 1,
        };
        let ready = jobs::record_source_verifier_boot_startup(&pool, &request).unwrap();
        assert_eq!(ready.ready_heartbeat_sequence, 2);
        let populated = pre068_rows(&pool.get().unwrap());
        let guards = external_trigger_snapshot(&pool.get().unwrap());
        for _ in 0..2 {
            db::run_migrations(&pool).unwrap();
            let conn = pool.get().unwrap();
            assert_connection_integrity(&conn);
            assert_legacy_rows(&conn, &populated);
            assert_eq!(external_trigger_snapshot(&conn), guards);
            drop(conn);
            let replay = jobs::record_source_verifier_boot_startup(&pool, &request).unwrap();
            assert!(replay.replayed);
            assert_eq!(
                replay.canonical_response_base64url,
                ready.canonical_response_base64url
            );
            assert_eq!(replay.response_sha256, ready.response_sha256);
            assert_eq!(replay.startup_at_ms, ready.startup_at_ms);
        }
    }

    #[test]
    fn c1_sqlite_populated_legacy_upgrade_is_atomic_and_preserves_signed_bytes() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let directory = ProvisioningTestDirectory::new();
        let source_path = directory.path().join("source.sqlite3");
        let source = db::open_pool(&source_path).unwrap();
        db::run_migrations(&source).unwrap();
        let unit = fixture(&source);
        let original_canonical = text(
            &source,
            "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations",
        );
        let target = db::open_pool(&directory.path().join("pre067.sqlite3")).unwrap();
        let startup_position = db::MIGRATIONS
            .iter()
            .position(|migration| *migration == db::SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP)
            .expect("the exact 067 migration must remain registered");
        db::run_sqlite_migration_list(&target, &db::MIGRATIONS[..startup_position]).unwrap();
        // Copy only normal unit-produced legacy authority rows into the actual pre-067 schema.
        // This does not invent a v4 proof or downgrade a v4 database.
        copy_normal_legacy_fixture_to_pre067(&source_path, &target, true);
        {
            let mut conn = target.get().unwrap();
            let columns: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('jobs_managed_cloud_activations') WHERE name='authority_kind'", [], |row| row.get(0)).unwrap();
            assert_eq!(columns, 0);
            const MARKER_INSERT: &str = "INSERT OR IGNORE INTO bluey_data_migrations(name)\nVALUES('jobs-provisioned-runtime-startup-storage-v1');";
            assert_eq!(
                db::SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP
                    .matches(MARKER_INSERT)
                    .count(),
                1
            );
            let interrupted = db::SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP.replacen(
                MARKER_INSERT,
                &format!("SELECT * FROM c1_upgrade_injected_failure;\n{MARKER_INSERT}"),
                1,
            );
            assert_ne!(interrupted, db::SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP);
            assert!(
                db::execute_sqlite_provisioned_runtime_migration(&mut conn, &interrupted).is_err()
            );
            assert!(conn.is_autocommit());
            assert!(conn
                .pragma_query_value::<bool, _>(None, "foreign_keys", |row| row.get(0))
                .unwrap());
            let columns: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('jobs_managed_cloud_activations') WHERE name='authority_kind'", [], |row| row.get(0)).unwrap();
            assert_eq!(
                columns, 0,
                "failed table rebuild must restore the populated legacy schema"
            );
            let new_tables: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('jobs_managed_cloud_queue_generations','jobs_managed_cloud_preproduction_contexts')", [], |row| row.get(0)).unwrap();
            assert_eq!(new_tables, 0);
        }
        assert_eq!(
            text(
                &target,
                "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations"
            ),
            original_canonical
        );
        db::run_migrations(&target).unwrap();
        assert_eq!(
            text(
                &target,
                "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations"
            ),
            original_canonical
        );
        assert_eq!(
            text(
                &target,
                "SELECT authority_kind FROM jobs_managed_cloud_activations"
            ),
            "legacy_activation"
        );
        assert_eq!(count(&target, "SELECT COUNT(*) FROM jobs_managed_cloud_activations WHERE purpose='production' AND context_sha256 IS NULL AND queue_generation_id IS NULL"), 1);
        let status =
            jobs::get_managed_cloud_release_status(&target, &unit.activation.scope).unwrap();
        assert_eq!(
            status.activation_sha256.as_deref(),
            Some(unit.legacy_sha256.as_str())
        );
        assert_eq!(status.head_revision, 1);
        assert!(execute(
            &target,
            "UPDATE jobs_managed_cloud_activations SET recorded_by='changed'"
        )
        .is_err());
        assert!(execute(&target, "DELETE FROM jobs_managed_cloud_head_transitions").is_err());
        db::run_migrations(&target).unwrap();
        assert_eq!(
            text(
                &target,
                "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations"
            ),
            original_canonical
        );
    }

    #[test]
    fn c1_sqlite_first_head_accepts_only_provisioning_kind_and_keeps_business_closed() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let directory = ProvisioningTestDirectory::new();
        let source_path = directory.path().join("normal-source.sqlite3");
        let source = db::open_pool(&source_path).unwrap();
        db::run_migrations(&source).unwrap();
        let unit = fixture(&source);
        let target = db::open_pool(&directory.path().join("first-head.sqlite3")).unwrap();
        let startup_position = db::MIGRATIONS
            .iter()
            .position(|migration| *migration == db::SQLITE_JOBS_PROVISIONED_RUNTIME_STARTUP)
            .expect("the exact 067 migration must remain registered");
        db::run_sqlite_migration_list(&target, &db::MIGRATIONS[..startup_position]).unwrap();
        // Import-only state: normal signed rows, with no historical head/transition copied.
        copy_normal_legacy_fixture_to_pre067(&source_path, &target, false);
        db::run_migrations(&target).unwrap();
        assert_eq!(
            count(&target, "SELECT COUNT(*) FROM jobs_managed_cloud_heads"),
            0
        );
        let mut doc = document(&unit);
        doc.expected_head_revision = 0;
        doc.expected_transition_sha256 = None;
        doc.predecessor_activation_sha256 = None;
        insert_queue(&target, &unit, &doc);
        let signed = signature(&target, &doc);
        execute(&target, &candidate_sql(&target, &unit, &doc, &signed, &[])).unwrap();
        let transition = managed_cloud_sha256(b"phase648-unit-only-first-head");
        let transition_sql = format!("INSERT INTO jobs_managed_cloud_head_transitions(transition_sha256,environment,region,channel,head_revision,previous_head_revision,next_activation_sha256,next_manifest_sha256,next_trust_generation,next_channel_sequence,transition_kind,authority_sha256,recorded_by,recorded_at_ms) VALUES({},'production','us-east-1','general',1,0,{},{},{},{},'provisioning',{},'phase648-unit-only',{});",
            quoted(&transition), quoted(&signed.0), quoted(&doc.manifest_sha256), doc.trust_generation,
            doc.channel_sequence, quoted(&signed.0), unit.now);
        let wrong_kind = transition_sql.replace("'provisioning'", "'activation'");
        assert!(execute(&target, &wrong_kind)
            .unwrap_err()
            .contains("kind mismatch"));
        execute(&target, &transition_sql).unwrap();
        execute(&target, &format!("INSERT INTO jobs_managed_cloud_heads(environment,region,channel,head_revision,current_transition_sha256,current_activation_sha256,current_manifest_sha256,current_trust_generation,current_channel_sequence,updated_at_ms) VALUES('production','us-east-1','general',1,{},{},{},{},{},{});",
            quoted(&transition), quoted(&signed.0), quoted(&doc.manifest_sha256), doc.trust_generation,
            doc.channel_sequence, unit.now)).unwrap();
        let readiness = jobs::resolve_managed_cloud_readiness(
            &target,
            &jobs::ManagedCloudReadinessQuery {
                scope: doc.scope.clone(),
                account_id: None,
            },
        )
        .unwrap();
        assert_eq!(
            readiness.status.activation_sha256.as_deref(),
            Some(signed.0.as_str())
        );
        assert_eq!(readiness.status.head_revision, 1);
        assert_eq!(
            readiness.status.unavailability_reason.as_deref(),
            Some("provisioned_nonserving")
        );
        assert!(!readiness.status.authority_ready && !readiness.status.customer_admission);
        assert!(jobs::managed_cloud_admission_from_readiness(&readiness).is_err());
    }

    fn postgres_045_failure_fixture(sql: &str) -> String {
        const ANCHOR: &str = "\nDO $migration$\n";
        const TAIL: &str =
            "USING '045_jobs_provisioned_runtime_startup.sql';\nEND;\n$migration$;\nCOMMIT;";
        assert_eq!(sql.matches(ANCHOR).count(), 1);
        assert_eq!(sql.matches(TAIL).count(), 1);
        assert!(sql.trim_end().ends_with(TAIL));
        sql.replacen(
            ANCHOR,
            "\nSELECT * FROM c1_injected_missing_relation;\nDO $migration$\n",
            1,
        )
    }

    #[test]
    fn c1_postgres_045_failure_fixture_anchors_actual_self_record_block() {
        let sql = db::POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP;
        let failure = postgres_045_failure_fixture(sql);
        assert_ne!(failure, sql);
        assert!(failure.contains("\nSELECT * FROM c1_injected_missing_relation;\nDO $migration$\n"));
    }

    fn postgres_c1_canonical_history(conn: &mut postgres::Client) -> Vec<(String, String)> {
        conn.query(
            "SELECT version,(applied_at AT TIME ZONE 'UTC')::text
               FROM public.bluey_schema_migrations ORDER BY version",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
    }

    fn postgres_c1_legacy_columns(conn: &mut postgres::Client) -> Vec<(String, Vec<String>)> {
        LEGACY_TABLES
            .iter()
            .map(|suffix| {
                let table = format!("jobs_managed_cloud_{suffix}");
                let columns = conn
                    .query(
                        "SELECT column_name FROM information_schema.columns
                  WHERE table_schema='public' AND table_name=$1 ORDER BY ordinal_position",
                        &[&table],
                    )
                    .unwrap()
                    .iter()
                    .map(|row| row.get::<_, String>(0))
                    .collect::<Vec<_>>();
                assert!(!columns.is_empty());
                assert!(columns.iter().all(|column| column
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_')));
                (table, columns)
            })
            .collect()
    }

    fn postgres_c1_legacy_rows(
        conn: &mut postgres::Client,
        columns: &[(String, Vec<String>)],
    ) -> Vec<String> {
        let mut values = Vec::new();
        for (table, names) in columns {
            for row in conn
                .query(
                    &format!(
                "SELECT row_to_json(legacy_row)::text FROM (SELECT {} FROM {table}) legacy_row
                 ORDER BY row_to_json(legacy_row)::text", names.join(",")),
                    &[],
                )
                .unwrap()
            {
                values.push(format!("{table}:{}", row.get::<_, String>(0)));
            }
        }
        values
    }

    fn postgres_c1_schema_snapshot(conn: &mut postgres::Client) -> Vec<String> {
        conn.query(
            "SELECT 'relation:' || row_to_json(value)::text FROM (
               SELECT c.relname,c.relkind,c.relpersistence,c.relrowsecurity,c.relforcerowsecurity,
                      c.relispartition FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
                WHERE n.nspname='public' AND c.relkind='r'
                  AND (left(c.relname,19)='jobs_managed_cloud_' OR c.relname='bluey_schema_migrations')
             ) value
             UNION ALL SELECT 'column:' || row_to_json(value)::text FROM (
               SELECT c.relname,a.attnum,a.attname,format_type(a.atttypid,a.atttypmod) AS data_type,
                      a.attnotnull,a.attisdropped,a.attidentity,a.attgenerated,
                      pg_get_expr(d.adbin,d.adrelid) AS default_expr
                 FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid
                 JOIN pg_namespace n ON n.oid=c.relnamespace
                 LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
                WHERE n.nspname='public' AND c.relkind='r' AND a.attnum>0
                  AND (left(c.relname,19)='jobs_managed_cloud_' OR c.relname='bluey_schema_migrations')
             ) value
             UNION ALL SELECT 'constraint:' || c.relname || ':' || con.conname || ':'
               || pg_get_constraintdef(con.oid) FROM pg_constraint con
               JOIN pg_class c ON c.oid=con.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND
                 (left(c.relname,19)='jobs_managed_cloud_' OR c.relname='bluey_schema_migrations')
             UNION ALL SELECT 'index:' || indexdef FROM pg_indexes
               WHERE schemaname='public' AND
                 (left(tablename,19)='jobs_managed_cloud_' OR tablename='bluey_schema_migrations')
             UNION ALL SELECT 'trigger:' || c.relname || ':' || t.tgenabled::text || ':'
               || pg_get_triggerdef(t.oid) FROM pg_trigger t
               JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND
                 (left(c.relname,19)='jobs_managed_cloud_' OR c.relname='bluey_schema_migrations')
             UNION ALL SELECT 'function:' || pg_get_functiondef(p.oid) FROM pg_proc p
               JOIN pg_namespace n ON n.oid=p.pronamespace
               WHERE n.nspname='public' AND left(p.proname,19)='jobs_managed_cloud_'
             ORDER BY 1",
            &[],
        ).unwrap().iter().map(|row| row.get(0)).collect()
    }

    fn assert_postgres_failed_upgrade_is_rolled_back(pool: &DbPool) {
        let mut conn = pool.get_pg().unwrap();
        let before_pid: i32 = conn
            .query_one("SELECT pg_backend_pid()", &[])
            .unwrap()
            .get(0);
        let before_guc: String = conn
            .query_one("SELECT current_setting('bluey.migration_table')", &[])
            .unwrap()
            .get(0);
        assert_eq!(before_guc, "phase655_ambient_ledger");
        let before_history = postgres_c1_canonical_history(&mut conn);
        assert!(!before_history.iter().any(|(version, _)| matches!(
            version.as_str(),
            "045_jobs_provisioned_runtime_startup.sql"
                | "046_jobs_provisioned_runtime_observations.sql"
        )));
        let columns = postgres_c1_legacy_columns(&mut conn);
        let before_legacy = postgres_c1_legacy_rows(&mut conn, &columns);
        let before_schema = postgres_c1_schema_snapshot(&mut conn);
        let missing: bool = conn
            .query_one(
                "SELECT to_regclass('public.c1_injected_missing_relation') IS NULL
                    AND to_regclass('public.phase655_ambient_ledger') IS NULL
                    AND to_regclass('public.jobs_managed_cloud_queue_generations') IS NULL
                    AND to_regclass('public.jobs_managed_cloud_preproduction_contexts') IS NULL
                    AND to_regclass('public.jobs_managed_cloud_bootstrap_sets') IS NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(missing, "the exact pre045 fault boundary must be absent");
        let before: String = conn.query_one("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='jobs_managed_cloud_signature_sets'::regclass AND conname='jobs_managed_cloud_signature_sets_target_audience_check'", &[]).unwrap().get(0);
        // Inject after the actual 045 DDL, immediately before its self-record block.
        let failure = postgres_045_failure_fixture(db::POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP);
        assert_ne!(failure, db::POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP);
        let error =
            db::execute_postgres_provisioned_runtime_migration(&mut conn, &failure).unwrap_err();
        assert!(error.to_string().contains("rollback=ok"));
        let database_error = error
            .chain()
            .find_map(|cause| {
                cause
                    .downcast_ref::<postgres::Error>()
                    .and_then(|error| error.as_db_error())
            })
            .expect("the anchored045 fault must retain its PostgreSQL cause");
        assert_eq!(
            database_error.code(),
            &postgres::error::SqlState::UNDEFINED_TABLE
        );
        assert!(database_error
            .message()
            .contains("c1_injected_missing_relation"));
        let after_guc: String = conn
            .query_one("SELECT current_setting('bluey.migration_table')", &[])
            .unwrap()
            .get(0);
        assert_eq!(
            after_guc, before_guc,
            "rollback must restore this checkout's ambient GUC"
        );
        let after_pid: i32 = conn
            .query_one("SELECT pg_backend_pid()", &[])
            .unwrap()
            .get(0);
        assert_eq!(
            after_pid, before_pid,
            "the private rollback proof holds one backend"
        );
        let ambient_absent: bool = conn
            .query_one(
                "SELECT to_regclass('public.phase655_ambient_ledger') IS NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(ambient_absent);
        assert_eq!(postgres_c1_canonical_history(&mut conn), before_history);
        assert_eq!(postgres_c1_legacy_rows(&mut conn, &columns), before_legacy);
        assert_eq!(postgres_c1_schema_snapshot(&mut conn), before_schema);
        let usable: i32 = conn
            .query_one("SELECT 1::integer", &[])
            .expect("failed migration must not return an aborted session")
            .get(0);
        assert_eq!(usable, 1);
        let after: String = conn.query_one("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='jobs_managed_cloud_signature_sets'::regclass AND conname='jobs_managed_cloud_signature_sets_target_audience_check'", &[]).unwrap().get(0);
        assert_eq!(after, before);
        let partial: bool = conn.query_one("SELECT to_regclass('jobs_managed_cloud_queue_generations') IS NOT NULL OR to_regclass('jobs_managed_cloud_preproduction_contexts') IS NOT NULL", &[]).unwrap().get(0);
        assert!(!partial);
        let marker: i64 = conn.query_one("SELECT COUNT(*) FROM bluey_schema_migrations WHERE version='045_jobs_provisioned_runtime_startup.sql'", &[]).unwrap().get(0);
        assert_eq!(marker, 0);
        let columns: i64 = conn.query_one("SELECT COUNT(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='jobs_managed_cloud_activations' AND column_name='authority_kind'", &[]).unwrap().get(0);
        assert_eq!(columns, 0);
    }

    fn copy_normal_legacy_fixture_to_pre045(source: &DbPool, target: &DbPool) {
        use rusqlite::types::ValueRef;
        let sqlite = source.get().unwrap();
        let mut postgres = target.get_pg().unwrap();
        let mut tx = postgres.transaction().unwrap();
        for suffix in LEGACY_TABLES {
            let table = format!("jobs_managed_cloud_{suffix}");
            let target_columns = tx
                .query(
                    "SELECT column_name,data_type FROM information_schema.columns
                WHERE table_schema='public' AND table_name=$1 ORDER BY ordinal_position",
                    &[&table],
                )
                .unwrap();
            let columns = target_columns
                .iter()
                .map(|row| row.get::<_, String>(0))
                .collect::<Vec<_>>();
            assert!(!columns.is_empty());
            assert!(columns.iter().all(|column| column
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')));
            let mut source_columns = sqlite
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            if *suffix == "activations" {
                source_columns.retain(|column| {
                    ![
                        "authority_kind",
                        "purpose",
                        "context_sha256",
                        "queue_generation_id",
                    ]
                    .contains(&column.as_str())
                });
            }
            let mut target_set = columns.clone();
            target_set.sort();
            source_columns.sort();
            assert_eq!(
                source_columns, target_set,
                "unit transfer column closure changed for {table}"
            );
            let mut statement = sqlite
                .prepare(&format!("SELECT {} FROM {table}", columns.join(",")))
                .unwrap();
            let mut rows = statement.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let values = target_columns
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let data_type: String = column.get(1);
                        match (data_type.as_str(), row.get_ref(index).unwrap()) {
                            (_, ValueRef::Null) => "NULL".into(),
                            ("boolean", ValueRef::Integer(0)) => "FALSE".into(),
                            ("boolean", ValueRef::Integer(1)) => "TRUE".into(),
                            ("bigint" | "integer", ValueRef::Integer(value)) => value.to_string(),
                            ("text", ValueRef::Text(value)) => {
                                quoted(std::str::from_utf8(value).unwrap())
                            }
                            _ => panic!("unexpected unit transfer value/type in {table}"),
                        }
                    })
                    .collect::<Vec<_>>();
                tx.batch_execute(&format!(
                    "INSERT INTO {table}({}) VALUES({});",
                    columns.join(","),
                    values.join(",")
                ))
                .unwrap();
            }
        }
        // All ordinary immediate FKs/triggers stayed enabled for this transfer.
        tx.commit().unwrap();
    }

    #[test]
    fn c1_sqlite_storage_constraints_and_nonserving_readers_preserve_v4_on_replay() {
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let pool = jobs::tests::test_pool();
        let unit = fixture(&pool);
        assert_constraints(&pool, unit);
        let conn = pool.get().unwrap();
        assert!(conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none());
    }

    #[test]
    fn c1_postgres_storage_upgrade_constraints_nonserving_and_replay_when_configured() {
        let Some(url) = crate::db::owned_postgres_test::owned_url(
            "db::jobs::managed_cloud_provisioning_persistence::tests::c1_postgres_storage_upgrade_constraints_nonserving_and_replay_when_configured",
        ) else {
            return;
        };
        let _guard = legacy_tests::postgres_boot_test_lock().lock().unwrap();
        let mut parsed = reqwest::Url::parse(&url).expect("validated owned C1 URL");
        parsed.set_query(Some(
            "sslmode=disable&options=-c%20bluey.migration_table%3Dphase655_ambient_ledger",
        ));
        let config = parsed.as_str().parse::<postgres::Config>().unwrap();
        assert_eq!(
            config.get_options(),
            Some("-c bluey.migration_table=phase655_ambient_ledger")
        );
        let pool = db::open_postgres_pool(parsed.as_str()).unwrap();
        {
            let mut conn = pool.get_pg().unwrap();
            let setting: String = conn
                .query_one("SELECT current_setting('bluey.migration_table')", &[])
                .unwrap()
                .get(0);
            assert_eq!(setting, "phase655_ambient_ledger");
        }
        let present: bool = pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(
            !present,
            "this case requires a separately owned fresh nonproduction database"
        );
        let startup_position = db::POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .position(|(version, _)| *version == db::JOBS_PROVISIONED_RUNTIME_STARTUP_MIGRATION_ID)
            .expect("the exact 045 migration must remain registered");
        db::run_postgres_migration_list(
            &pool,
            &db::POSTGRES_POST_JOBS_MIGRATIONS[..startup_position],
        )
        .unwrap();
        let directory = ProvisioningTestDirectory::new();
        let source = db::open_pool(&directory.path().join("normal-unit-source.sqlite3")).unwrap();
        db::run_migrations(&source).unwrap();
        let unit = fixture(&source);
        let original = text(
            &source,
            "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations",
        );
        copy_normal_legacy_fixture_to_pre045(&source, &pool);
        let (legacy_columns, legacy_rows) = {
            let mut conn = pool.get_pg().unwrap();
            let columns = postgres_c1_legacy_columns(&mut conn);
            let rows = postgres_c1_legacy_rows(&mut conn, &columns);
            (columns, rows)
        };
        let prior_foreign_keys = pool.get_pg().unwrap().query("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid IN ('jobs_managed_cloud_activations'::regclass,'jobs_managed_cloud_head_transitions'::regclass) AND contype='f'", &[]).unwrap()
            .iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
        let prior_indexes = pool.get_pg().unwrap().query("SELECT indexdef FROM pg_indexes WHERE schemaname='public' AND tablename IN ('jobs_managed_cloud_activations','jobs_managed_cloud_head_transitions','jobs_managed_cloud_signature_sets')", &[]).unwrap()
            .iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
        assert_eq!(
            text(
                &pool,
                "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations"
            ),
            original
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema='public' AND table_name='jobs_managed_cloud_activations' AND column_name='authority_kind'"), 0);
        assert_postgres_failed_upgrade_is_rolled_back(&pool);
        db::run_migrations(&pool).unwrap();
        let upgraded_history = {
            let mut conn = pool.get_pg().unwrap();
            let setting: String = conn
                .query_one("SELECT current_setting('bluey.migration_table')", &[])
                .unwrap()
                .get(0);
            assert_eq!(setting, "phase655_ambient_ledger");
            let schema: bool = conn.query_one(
                "SELECT to_regclass('public.phase655_ambient_ledger') IS NULL
                        AND to_regclass('public.jobs_managed_cloud_preproduction_contexts') IS NOT NULL
                        AND to_regclass('public.jobs_managed_cloud_bootstrap_sets') IS NOT NULL", &[],
            ).unwrap().get(0);
            assert!(schema);
            assert_eq!(
                postgres_c1_legacy_rows(&mut conn, &legacy_columns),
                legacy_rows
            );
            let history = postgres_c1_canonical_history(&mut conn);
            for version in [
                "045_jobs_provisioned_runtime_startup.sql",
                "046_jobs_provisioned_runtime_observations.sql",
            ] {
                assert_eq!(
                    history
                        .iter()
                        .filter(|(actual, _)| actual == version)
                        .count(),
                    1
                );
            }
            history
        };
        let current_foreign_keys = pool.get_pg().unwrap().query("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid IN ('jobs_managed_cloud_activations'::regclass,'jobs_managed_cloud_head_transitions'::regclass) AND contype='f'", &[]).unwrap()
            .iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
        let current_indexes = pool.get_pg().unwrap().query("SELECT indexdef FROM pg_indexes WHERE schemaname='public' AND tablename IN ('jobs_managed_cloud_activations','jobs_managed_cloud_head_transitions','jobs_managed_cloud_signature_sets')", &[]).unwrap()
            .iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
        assert!(!prior_foreign_keys.is_empty() && !prior_indexes.is_empty());
        assert!(prior_foreign_keys
            .iter()
            .all(|value| current_foreign_keys.contains(value)));
        assert!(prior_indexes
            .iter()
            .all(|value| current_indexes.contains(value)));
        assert_eq!(
            text(
                &pool,
                "SELECT canonical_activation_base64url FROM jobs_managed_cloud_activations"
            ),
            original
        );
        assert_eq!(
            text(
                &pool,
                "SELECT authority_kind FROM jobs_managed_cloud_activations"
            ),
            "legacy_activation"
        );
        assert_eq!(
            text(
                &pool,
                "SELECT current_transition_sha256 FROM jobs_managed_cloud_heads"
            ),
            unit.transition_sha256
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM jobs_managed_cloud_head_transitions"
            ),
            1
        );
        assert!(execute(
            &pool,
            "UPDATE jobs_managed_cloud_activations SET recorded_by='changed'"
        )
        .is_err());
        assert!(execute(&pool, "DELETE FROM jobs_managed_cloud_head_transitions").is_err());
        assert_constraints(&pool, unit);
        let (replay_columns, replay_legacy, replay_schema) = {
            let mut conn = pool.get_pg().unwrap();
            let columns = postgres_c1_legacy_columns(&mut conn);
            let legacy = postgres_c1_legacy_rows(&mut conn, &columns);
            let schema = postgres_c1_schema_snapshot(&mut conn);
            (columns, legacy, schema)
        };
        db::run_migrations(&pool).unwrap();
        {
            let mut conn = pool.get_pg().unwrap();
            assert_eq!(postgres_c1_canonical_history(&mut conn), upgraded_history);
            assert_eq!(
                postgres_c1_legacy_rows(&mut conn, &replay_columns),
                replay_legacy
            );
            assert_eq!(postgres_c1_schema_snapshot(&mut conn), replay_schema);
            let setting: String = conn
                .query_one("SELECT current_setting('bluey.migration_table')", &[])
                .unwrap()
                .get(0);
            assert_eq!(setting, "phase655_ambient_ledger");
            let absent: bool = conn
                .query_one(
                    "SELECT to_regclass('public.phase655_ambient_ledger') IS NULL",
                    &[],
                )
                .unwrap()
                .get(0);
            assert!(absent);
        }
        // Existing cfg(test) transaction primitive, not late045 selected-driver proof.
        println!("phase657_owned_c1_ambient_late045_primitive_rollback_complete");
        println!("phase655_owned_c1_postgres_persistence_complete");
    }
}
