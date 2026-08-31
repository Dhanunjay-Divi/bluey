use ed25519_dalek::{
    Signature as Ed25519Signature, Signer as Ed25519SignerTrait, SigningKey as Ed25519SigningKey,
    Verifier as Ed25519VerifierTrait, VerifyingKey as Ed25519VerifyingKey,
};
use rusqlite::Transaction as RunnerSqliteTransaction;

pub const RUNNER_PURGE_COMMAND_VERSION: i64 = 2;
pub const RUNNER_PURGE_ACK_VERSION: i64 = 2;
pub const RUNNER_PURGE_STORAGE_EVIDENCE_VERSION: i64 = 2;
pub const RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION: i64 = 2;
pub const RUNNER_LEGACY_INVENTORY_VERSION: i64 = 1;
pub const RUNNER_STORAGE_ATTESTATION_VERSION: i64 = 1;
pub const RUNNER_PURGE_COMMAND_AUDIENCE: &str = "bluey-jobs-runner-volume-purge-v2";
pub const RUNNER_PURGE_ACK_AUDIENCE: &str = "bluey-jobs-runner-volume-purge-ack-v2";
pub const RUNNER_STORAGE_ATTESTATION_AUDIENCE: &str =
    "bluey-jobs-runner-volume-storage-attestation-v1";
pub const EMPTY_RUNNER_INVENTORY_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
pub const EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256: &str =
    "a1adfd38aaf5aeba65d751a82f1fc65d6091b34cea8ff7b9ecbee2c907b8c216";
pub const EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256: &str =
    "b414032476a3243362451c3a47d7989a9d84881436cb2f32e85b90098a4a0a1e";
pub const ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256: &str =
    "921610565c02d955bcfedb5df4a23b9c29069c711e8eec1399c481a88960af34";
pub const EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256: &str =
    "1d657a1abf311316ec1f0390d2fd76239cd5b06280325b670b4295a57096045c";
pub const GENESIS_RUNNER_STORAGE_ATTESTATION_SHA256: &str =
    "af14da54b5862fedcda27f7b1ca9ccd2be4271870efa7707d6f2e308efd874c2";
pub const EMPTY_RUNNER_STORAGE_ATTESTATION_SET_SHA256: &str =
    "76f460646d1be73e8796ece72560c26b7be276c4183fe569d1763b59f89ce1e9";

const RUNNER_PURGE_COMMAND_DOMAIN: &str = "bluey-jobs-runner-volume-purge-command-v2";
const RUNNER_PURGE_ACK_DOMAIN: &str = "bluey-jobs-runner-volume-purge-ack-v2";
const RUNNER_STORAGE_ATTESTATION_DOMAIN: &str = "bluey-jobs-runner-volume-storage-attestation-v1";
const RUNNER_STORAGE_ATTESTATION_SET_DOMAIN: &str =
    "bluey-jobs-runner-volume-storage-attestation-set-v1";
const RUNNER_PURGE_STORAGE_EVIDENCE_DOMAIN: &str = "bluey-jobs-runner-purge-storage-evidence-v2";
const RUNNER_PURGE_TARGET_INVENTORY_DOMAIN: &str = "bluey-jobs-runner-purge-target-inventory-v2";
const RUNNER_VOLUME_ENROLLMENT_DOMAIN: &str = "bluey-jobs-runner-volume-enrollment-v1";
const RUNNER_VOLUME_AUTHORITY_DOMAIN: &str = "bluey-jobs-runner-volume-authority-v1";
const RUNNER_PROCESS_RUNTIME_DOMAIN: &str = "bluey-jobs-runner-process-runtime-v1";
const RUNNER_LEGACY_INVENTORY_AUTHORITY_DOMAIN: &str =
    "bluey-jobs-runner-legacy-inventory-authority-v1";
const RUNNER_LEGACY_INVENTORY_ID_DOMAIN: &str =
    "bluey-jobs-runner-legacy-inventory-authority-id-v1";
pub const RUNNER_VOLUME_AUTHORITY_AUDIENCE: &str = "bluey-jobs-runner-volume-authority";
const RUNNER_TARGET_SET_DOMAIN: &str = "bluey-jobs-runner-purge-target-set-v1";
const RUNNER_VOLUME_ID_DOMAIN: &[u8] = b"bluey-jobs-runner\0volume-id-v1\0";
const MAX_RUNNER_PURGE_POLL_COMMANDS: usize = 100;
const MAX_RUNNER_IDENTIFIER_BYTES: usize = 128;
const MAX_JAVASCRIPT_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const RUNNER_STORAGE_ATTESTATION_MAX_FUTURE_MS: i64 = 300_000;

pub type RunnerVolumePurgeResult<T> = std::result::Result<T, RunnerVolumePurgeError>;

#[derive(Debug, Error)]
pub enum RunnerVolumePurgeError {
    #[error("Invalid runner-volume purge request.")]
    InvalidRequest,
    #[error("Runner-volume authority was not found.")]
    NotFound,
    #[error("Runner-volume authority conflicts with stored state.")]
    Conflict,
    #[error("Runner-volume signature or admission authority is invalid.")]
    Unauthorized,
    #[error("Runner-volume purge is not ready.")]
    NotReady,
    #[error("runner-volume purge storage failed")]
    Storage(#[source] anyhow::Error),
}

impl From<rusqlite::Error> for RunnerVolumePurgeError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<postgres::Error> for RunnerVolumePurgeError {
    fn from(error: postgres::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<anyhow::Error> for RunnerVolumePurgeError {
    fn from(error: anyhow::Error) -> Self {
        Self::Storage(error)
    }
}

#[derive(Clone)]
pub struct RunnerPurgeSigner {
    key_id: String,
    signing_key: Ed25519SigningKey,
}

impl RunnerPurgeSigner {
    /// Constructs a signer from durable key material supplied by the caller.
    /// This module deliberately never reads signing keys from the environment.
    pub fn from_seed(key_id: impl Into<String>, seed: [u8; 32]) -> RunnerVolumePurgeResult<Self> {
        let key_id = key_id.into();
        require_runner_identifier(&key_id)?;
        Ok(Self {
            key_id,
            signing_key: Ed25519SigningKey::from_bytes(&seed),
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn public_key_base64url(&self) -> String {
        encode_base64url(self.signing_key.verifying_key().as_bytes())
    }

    pub fn sign_command(
        &self,
        input: NewRunnerPurgeCommand,
    ) -> RunnerVolumePurgeResult<RunnerPurgeCommand> {
        let mut command = RunnerPurgeCommand {
            version: RUNNER_PURGE_COMMAND_VERSION,
            request_id: input.request_id,
            audience: RUNNER_PURGE_COMMAND_AUDIENCE.to_string(),
            command_id: input.command_id,
            target_volume_id: input.target_volume_id,
            target_key_fingerprint: input.target_key_fingerprint,
            enrollment_epoch: input.enrollment_epoch,
            purge_subject: input.purge_subject,
            purge_generation: input.purge_generation,
            storage_evidence_version: input.storage_evidence_version,
            subject_storage_layout_version: input.subject_storage_layout_version,
            legacy_inventory_authority_generation: input.legacy_inventory_authority_generation,
            legacy_inventory_authority_sha256: input.legacy_inventory_authority_sha256,
            issued_at_ms: input.issued_at_ms,
            minimum_runner_build_id: input.minimum_runner_build_id,
            server_key_id: self.key_id.clone(),
            signature: String::new(),
        };
        command.validate_unsigned()?;
        command.signature = encode_base64url(
            &self
                .signing_key
                .sign(&command.canonical_unsigned_bytes()?)
                .to_bytes(),
        );
        Ok(command)
    }

    pub fn verify_command(&self, command: &RunnerPurgeCommand) -> RunnerVolumePurgeResult<()> {
        if command.server_key_id != self.key_id {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        verify_runner_purge_command(command, &self.public_key_base64url())
    }
}

/// Trusted verification keys for persisted server-signed purge commands.
///
/// The current signer must be present with the exact matching public key, while
/// retired signing keys remain in the map until no durable command references
/// them. This lets replay, poll, and ACK paths authenticate historical commands
/// after a signer rotation instead of trusting mutable database columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPurgeCommandKeyRing {
    current_key_id: String,
    public_keys_by_id: BTreeMap<String, String>,
}

impl RunnerPurgeCommandKeyRing {
    pub fn new(
        current_signer: &RunnerPurgeSigner,
        public_keys_by_id: BTreeMap<String, String>,
    ) -> RunnerVolumePurgeResult<Self> {
        if public_keys_by_id.is_empty() {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        for (key_id, public_key) in &public_keys_by_id {
            require_runner_identifier(key_id)?;
            require_base64url(public_key, 32)?;
        }
        if public_keys_by_id.get(current_signer.key_id())
            != Some(&current_signer.public_key_base64url())
        {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        Ok(Self {
            current_key_id: current_signer.key_id().to_string(),
            public_keys_by_id,
        })
    }

    pub fn current_key_id(&self) -> &str {
        &self.current_key_id
    }

    pub fn public_keys_by_id(&self) -> &BTreeMap<String, String> {
        &self.public_keys_by_id
    }

    fn require_current_signer(&self, signer: &RunnerPurgeSigner) -> RunnerVolumePurgeResult<()> {
        if self.current_key_id != signer.key_id()
            || self.public_keys_by_id.get(signer.key_id()) != Some(&signer.public_key_base64url())
        {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        Ok(())
    }

    pub fn verify_command(&self, command: &RunnerPurgeCommand) -> RunnerVolumePurgeResult<()> {
        let public_key = self
            .public_keys_by_id
            .get(&command.server_key_id)
            .ok_or(RunnerVolumePurgeError::Unauthorized)?;
        verify_runner_purge_command(command, public_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerPurgeCommand {
    pub request_id: String,
    pub command_id: String,
    pub target_volume_id: String,
    pub target_key_fingerprint: String,
    pub enrollment_epoch: i64,
    pub purge_subject: String,
    pub purge_generation: i64,
    pub storage_evidence_version: i64,
    pub subject_storage_layout_version: i64,
    pub legacy_inventory_authority_generation: i64,
    pub legacy_inventory_authority_sha256: String,
    pub issued_at_ms: i64,
    pub minimum_runner_build_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeCommand {
    pub version: i64,
    pub request_id: String,
    pub audience: String,
    pub command_id: String,
    pub target_volume_id: String,
    pub target_key_fingerprint: String,
    pub enrollment_epoch: i64,
    pub purge_subject: String,
    pub purge_generation: i64,
    pub storage_evidence_version: i64,
    pub subject_storage_layout_version: i64,
    pub legacy_inventory_authority_generation: i64,
    pub legacy_inventory_authority_sha256: String,
    pub issued_at_ms: i64,
    pub minimum_runner_build_id: String,
    pub server_key_id: String,
    pub signature: String,
}

impl RunnerPurgeCommand {
    fn validate_unsigned(&self) -> RunnerVolumePurgeResult<()> {
        if self.version != RUNNER_PURGE_COMMAND_VERSION
            || self.audience != RUNNER_PURGE_COMMAND_AUDIENCE
            || self.enrollment_epoch <= 0
            || self.purge_generation <= 0
            || self.storage_evidence_version != RUNNER_PURGE_STORAGE_EVIDENCE_VERSION
            || self.subject_storage_layout_version != RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION
            || self.issued_at_ms < 0
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.request_id)?;
        require_runner_identifier(&self.command_id)?;
        require_base64url(&self.target_volume_id, 32)?;
        require_sha256(&self.target_key_fingerprint)?;
        require_base64url(&self.purge_subject, 32)?;
        validate_legacy_inventory_authority_binding(
            self.legacy_inventory_authority_generation,
            &self.legacy_inventory_authority_sha256,
        )?;
        if !runner_build_id_is_canonical(&self.minimum_runner_build_id) {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.server_key_id)
    }

    /// Exact byte contract implemented by `canonicalRunnerVolumePurgeCommand`
    /// in the runner. The final newline is part of the signature and digest.
    pub fn canonical_unsigned_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate_unsigned()?;
        Ok(format!(
            concat!(
                "{}\n",
                "version={}\n",
                "request_id={}\n",
                "audience={}\n",
                "command_id={}\n",
                "target_volume_id={}\n",
                "target_key_fingerprint={}\n",
                "enrollment_epoch={}\n",
                "purge_subject={}\n",
                "purge_generation={}\n",
                "storage_evidence_version={}\n",
                "subject_storage_layout_version={}\n",
                "legacy_inventory_authority_generation={}\n",
                "legacy_inventory_authority_sha256={}\n",
                "issued_at_ms={}\n",
                "minimum_runner_build_id={}\n",
                "server_key_id={}\n"
            ),
            RUNNER_PURGE_COMMAND_DOMAIN,
            self.version,
            self.request_id,
            self.audience,
            self.command_id,
            self.target_volume_id,
            self.target_key_fingerprint,
            self.enrollment_epoch,
            self.purge_subject,
            self.purge_generation,
            self.storage_evidence_version,
            self.subject_storage_layout_version,
            self.legacy_inventory_authority_generation,
            self.legacy_inventory_authority_sha256,
            self.issued_at_ms,
            self.minimum_runner_build_id,
            self.server_key_id,
        )
        .into_bytes())
    }

    /// The runner hashes the exact unsigned canonical command, not its JSON
    /// representation and not its signature. This binds ACKs across languages.
    pub fn command_sha256(&self) -> RunnerVolumePurgeResult<String> {
        require_base64url(&self.signature, 64)?;
        Ok(hex::encode(Sha256::digest(
            self.canonical_unsigned_bytes()?,
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeInventoryEvidence {
    pub entry_count: i64,
    pub file_bytes: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeRootSnapshotEvidence {
    pub link_count: i64,
    pub entry_count: i64,
    pub file_bytes: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeRootEvidence {
    pub device_id: String,
    pub before: RunnerPurgeRootSnapshotEvidence,
    pub after: RunnerPurgeRootSnapshotEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeLocatorEvidence {
    pub count: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeSubjectStorageSnapshotEvidence {
    pub residency: String,
    pub subject_tree: RunnerPurgeInventoryEvidence,
    pub ownership: RunnerPurgeInventoryEvidence,
    pub target: RunnerPurgeInventoryEvidence,
    pub complete_root: RunnerPurgeInventoryEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeSubjectStorageEvidence {
    pub layout_version: i64,
    pub before: RunnerPurgeSubjectStorageSnapshotEvidence,
    pub after: RunnerPurgeSubjectStorageSnapshotEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeLegacyRootSnapshotEvidence {
    pub artifact_count: i64,
    pub artifact_bytes: String,
    pub artifact_set_sha256: String,
    pub unclassified_root_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeLegacyEvidence {
    pub inventory_version: i64,
    pub target_before: RunnerPurgeInventoryEvidence,
    pub target_after: RunnerPurgeInventoryEvidence,
    pub root_before: RunnerPurgeLegacyRootSnapshotEvidence,
    pub root_after: RunnerPurgeLegacyRootSnapshotEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeStorageEvidence {
    pub version: i64,
    pub root: RunnerPurgeRootEvidence,
    pub locators: RunnerPurgeLocatorEvidence,
    pub subject_storage: RunnerPurgeSubjectStorageEvidence,
    pub legacy: RunnerPurgeLegacyEvidence,
}

impl RunnerPurgeStorageEvidence {
    fn validate(&self) -> RunnerVolumePurgeResult<()> {
        if self.version != RUNNER_PURGE_STORAGE_EVIDENCE_VERSION
            || self.subject_storage.layout_version != RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION
            || self.legacy.inventory_version != RUNNER_LEGACY_INVENTORY_VERSION
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.root.device_id)?;
        validate_root_snapshot(&self.root.before)?;
        validate_root_snapshot(&self.root.after)?;
        validate_nonnegative_safe_integer(self.locators.count)?;
        require_sha256(&self.locators.sha256)?;
        validate_subject_storage_snapshot(&self.subject_storage.before)?;
        validate_subject_storage_snapshot(&self.subject_storage.after)?;
        validate_inventory_evidence(&self.legacy.target_before, EMPTY_RUNNER_INVENTORY_SHA256)?;
        validate_inventory_evidence(&self.legacy.target_after, EMPTY_RUNNER_INVENTORY_SHA256)?;
        validate_legacy_root_snapshot(&self.legacy.root_before)?;
        validate_legacy_root_snapshot(&self.legacy.root_after)?;

        if (self.subject_storage.before.residency == "resident"
            && self.subject_storage.before.ownership.entry_count != self.locators.count)
            || (self.subject_storage.before.residency == "never_resident"
                && self.subject_storage.before.ownership.entry_count != 0)
            || self.subject_storage.after.residency != "never_resident"
            || !is_empty_inventory(
                &self.subject_storage.after.subject_tree,
                EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256,
            )
            || !is_empty_inventory(
                &self.subject_storage.after.ownership,
                EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256,
            )
            || !is_empty_inventory(
                &self.subject_storage.after.target,
                EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256,
            )
            || self.legacy.root_before.unclassified_root_count != 0
            || !is_empty_inventory(&self.legacy.target_after, EMPTY_RUNNER_INVENTORY_SHA256)
            || !is_empty_legacy_root_snapshot(&self.legacy.root_after)
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }

        validate_snapshot_subsets(
            &self.root.before,
            &self.subject_storage.before.complete_root,
            &self.legacy.root_before,
        )?;
        validate_snapshot_subsets(
            &self.root.after,
            &self.subject_storage.after.complete_root,
            &self.legacy.root_after,
        )?;
        validate_inventory_subset(
            &self.legacy.target_before,
            self.legacy.root_before.artifact_count,
            &self.legacy.root_before.artifact_bytes,
        )?;
        validate_inventory_subset(
            &self.legacy.target_after,
            self.legacy.root_after.artifact_count,
            &self.legacy.root_after.artifact_bytes,
        )?;
        Ok(())
    }

    /// Strict cross-language byte contract for `storageEvidenceSha256`.
    /// The final newline and every decimal string are part of the digest.
    pub fn canonical_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate()?;
        let before = &self.subject_storage.before;
        let after = &self.subject_storage.after;
        Ok(format!(
            concat!(
                "{}\n",
                "version={}\n",
                "root_device_id={}\n",
                "root_before_link_count={}\n",
                "root_before_entry_count={}\n",
                "root_before_file_bytes={}\n",
                "root_before_sha256={}\n",
                "root_after_link_count={}\n",
                "root_after_entry_count={}\n",
                "root_after_file_bytes={}\n",
                "root_after_sha256={}\n",
                "locators_count={}\n",
                "locators_sha256={}\n",
                "subject_storage_layout_version={}\n",
                "subject_storage_before_residency={}\n",
                "subject_storage_before_subject_tree_entry_count={}\n",
                "subject_storage_before_subject_tree_file_bytes={}\n",
                "subject_storage_before_subject_tree_sha256={}\n",
                "subject_storage_before_ownership_entry_count={}\n",
                "subject_storage_before_ownership_file_bytes={}\n",
                "subject_storage_before_ownership_sha256={}\n",
                "subject_storage_before_target_entry_count={}\n",
                "subject_storage_before_target_file_bytes={}\n",
                "subject_storage_before_target_sha256={}\n",
                "subject_storage_before_complete_root_entry_count={}\n",
                "subject_storage_before_complete_root_file_bytes={}\n",
                "subject_storage_before_complete_root_sha256={}\n",
                "subject_storage_after_residency={}\n",
                "subject_storage_after_subject_tree_entry_count={}\n",
                "subject_storage_after_subject_tree_file_bytes={}\n",
                "subject_storage_after_subject_tree_sha256={}\n",
                "subject_storage_after_ownership_entry_count={}\n",
                "subject_storage_after_ownership_file_bytes={}\n",
                "subject_storage_after_ownership_sha256={}\n",
                "subject_storage_after_target_entry_count={}\n",
                "subject_storage_after_target_file_bytes={}\n",
                "subject_storage_after_target_sha256={}\n",
                "subject_storage_after_complete_root_entry_count={}\n",
                "subject_storage_after_complete_root_file_bytes={}\n",
                "subject_storage_after_complete_root_sha256={}\n",
                "legacy_inventory_version={}\n",
                "legacy_target_before_entry_count={}\n",
                "legacy_target_before_file_bytes={}\n",
                "legacy_target_before_sha256={}\n",
                "legacy_target_after_entry_count={}\n",
                "legacy_target_after_file_bytes={}\n",
                "legacy_target_after_sha256={}\n",
                "legacy_root_before_artifact_count={}\n",
                "legacy_root_before_artifact_bytes={}\n",
                "legacy_root_before_artifact_set_sha256={}\n",
                "legacy_root_before_unclassified_root_count={}\n",
                "legacy_root_after_artifact_count={}\n",
                "legacy_root_after_artifact_bytes={}\n",
                "legacy_root_after_artifact_set_sha256={}\n",
                "legacy_root_after_unclassified_root_count={}\n"
            ),
            RUNNER_PURGE_STORAGE_EVIDENCE_DOMAIN,
            self.version,
            self.root.device_id,
            self.root.before.link_count,
            self.root.before.entry_count,
            self.root.before.file_bytes,
            self.root.before.sha256,
            self.root.after.link_count,
            self.root.after.entry_count,
            self.root.after.file_bytes,
            self.root.after.sha256,
            self.locators.count,
            self.locators.sha256,
            self.subject_storage.layout_version,
            before.residency,
            before.subject_tree.entry_count,
            before.subject_tree.file_bytes,
            before.subject_tree.sha256,
            before.ownership.entry_count,
            before.ownership.file_bytes,
            before.ownership.sha256,
            before.target.entry_count,
            before.target.file_bytes,
            before.target.sha256,
            before.complete_root.entry_count,
            before.complete_root.file_bytes,
            before.complete_root.sha256,
            after.residency,
            after.subject_tree.entry_count,
            after.subject_tree.file_bytes,
            after.subject_tree.sha256,
            after.ownership.entry_count,
            after.ownership.file_bytes,
            after.ownership.sha256,
            after.target.entry_count,
            after.target.file_bytes,
            after.target.sha256,
            after.complete_root.entry_count,
            after.complete_root.file_bytes,
            after.complete_root.sha256,
            self.legacy.inventory_version,
            self.legacy.target_before.entry_count,
            self.legacy.target_before.file_bytes,
            self.legacy.target_before.sha256,
            self.legacy.target_after.entry_count,
            self.legacy.target_after.file_bytes,
            self.legacy.target_after.sha256,
            self.legacy.root_before.artifact_count,
            self.legacy.root_before.artifact_bytes,
            self.legacy.root_before.artifact_set_sha256,
            self.legacy.root_before.unclassified_root_count,
            self.legacy.root_after.artifact_count,
            self.legacy.root_after.artifact_bytes,
            self.legacy.root_after.artifact_set_sha256,
            self.legacy.root_after.unclassified_root_count,
        )
        .into_bytes())
    }

    pub fn sha256(&self) -> RunnerVolumePurgeResult<String> {
        Ok(hex::encode(Sha256::digest(self.canonical_bytes()?)))
    }

    fn target_inventory_state(&self, before: bool) -> RunnerVolumePurgeResult<(i64, String)> {
        let (legacy, subject) = if before {
            (
                &self.legacy.target_before,
                &self.subject_storage.before.target,
            )
        } else {
            (
                &self.legacy.target_after,
                &self.subject_storage.after.target,
            )
        };
        let entry_count = legacy
            .entry_count
            .checked_add(subject.entry_count)
            .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
        validate_nonnegative_safe_integer(entry_count)?;
        if entry_count == 0 {
            return Ok((0, EMPTY_RUNNER_INVENTORY_SHA256.to_string()));
        }
        let canonical = format!(
            concat!(
                "{}\n",
                "legacy_entry_count={}\n",
                "legacy_file_bytes={}\n",
                "legacy_sha256={}\n",
                "subject_entry_count={}\n",
                "subject_file_bytes={}\n",
                "subject_sha256={}\n"
            ),
            RUNNER_PURGE_TARGET_INVENTORY_DOMAIN,
            legacy.entry_count,
            legacy.file_bytes,
            legacy.sha256,
            subject.entry_count,
            subject.file_bytes,
            subject.sha256,
        );
        Ok((entry_count, hex::encode(Sha256::digest(canonical))))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerPurgeAck {
    pub request_id: String,
    pub command_id: String,
    pub command_sha256: String,
    pub target_volume_id: String,
    pub target_key_fingerprint: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub purge_subject_sha256: String,
    pub purge_generation: i64,
    pub storage_evidence: RunnerPurgeStorageEvidence,
    pub storage_evidence_sha256: String,
    pub before_inventory_count: i64,
    pub before_inventory_sha256: String,
    pub after_inventory_count: i64,
    pub after_inventory_sha256: String,
    pub removed_count: i64,
    pub runner_build_id: String,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeAck {
    pub version: i64,
    pub audience: String,
    pub request_id: String,
    pub command_id: String,
    pub command_sha256: String,
    pub target_volume_id: String,
    pub target_key_fingerprint: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub purge_subject_sha256: String,
    pub purge_generation: i64,
    pub storage_evidence: RunnerPurgeStorageEvidence,
    pub storage_evidence_sha256: String,
    pub before_inventory_count: i64,
    pub before_inventory_sha256: String,
    pub after_inventory_count: i64,
    pub after_inventory_sha256: String,
    pub removed_count: i64,
    pub runner_build_id: String,
    pub completed_at_ms: i64,
    pub signature: String,
}

impl RunnerPurgeAck {
    fn from_unsigned(input: NewRunnerPurgeAck) -> Self {
        Self {
            version: RUNNER_PURGE_ACK_VERSION,
            audience: RUNNER_PURGE_ACK_AUDIENCE.to_string(),
            request_id: input.request_id,
            command_id: input.command_id,
            command_sha256: input.command_sha256,
            target_volume_id: input.target_volume_id,
            target_key_fingerprint: input.target_key_fingerprint,
            enrollment_epoch: input.enrollment_epoch,
            process_instance_id: input.process_instance_id,
            purge_subject_sha256: input.purge_subject_sha256,
            purge_generation: input.purge_generation,
            storage_evidence: input.storage_evidence,
            storage_evidence_sha256: input.storage_evidence_sha256,
            before_inventory_count: input.before_inventory_count,
            before_inventory_sha256: input.before_inventory_sha256,
            after_inventory_count: input.after_inventory_count,
            after_inventory_sha256: input.after_inventory_sha256,
            removed_count: input.removed_count,
            runner_build_id: input.runner_build_id,
            completed_at_ms: input.completed_at_ms,
            signature: String::new(),
        }
    }

    fn validate_unsigned(&self) -> RunnerVolumePurgeResult<()> {
        self.storage_evidence.validate()?;
        let before_inventory = self.storage_evidence.target_inventory_state(true)?;
        let after_inventory = self.storage_evidence.target_inventory_state(false)?;
        if self.version != RUNNER_PURGE_ACK_VERSION
            || self.audience != RUNNER_PURGE_ACK_AUDIENCE
            || self.enrollment_epoch <= 0
            || self.purge_generation <= 0
            || self.storage_evidence.sha256()? != self.storage_evidence_sha256
            || (
                self.before_inventory_count,
                self.before_inventory_sha256.as_str(),
            ) != (before_inventory.0, before_inventory.1.as_str())
            || (
                self.after_inventory_count,
                self.after_inventory_sha256.as_str(),
            ) != (after_inventory.0, after_inventory.1.as_str())
            || self.after_inventory_count != 0
            || self.removed_count
                != self
                    .before_inventory_count
                    .checked_sub(self.after_inventory_count)
                    .ok_or(RunnerVolumePurgeError::InvalidRequest)?
            || self.completed_at_ms < 0
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.request_id)?;
        require_runner_identifier(&self.command_id)?;
        require_sha256(&self.command_sha256)?;
        require_base64url(&self.target_volume_id, 32)?;
        require_sha256(&self.target_key_fingerprint)?;
        require_base64url(&self.process_instance_id, 32)?;
        require_sha256(&self.purge_subject_sha256)?;
        require_sha256(&self.storage_evidence_sha256)?;
        require_sha256(&self.before_inventory_sha256)?;
        require_sha256(&self.after_inventory_sha256)?;
        if !runner_build_id_is_canonical(&self.runner_build_id) {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        Ok(())
    }

    /// Exact byte contract implemented by `canonicalRunnerVolumePurgeAck` in
    /// the runner. The final newline is part of the signature.
    pub fn canonical_unsigned_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate_unsigned()?;
        Ok(format!(
            concat!(
                "{}\n",
                "version={}\n",
                "audience={}\n",
                "request_id={}\n",
                "command_id={}\n",
                "command_sha256={}\n",
                "target_volume_id={}\n",
                "target_key_fingerprint={}\n",
                "enrollment_epoch={}\n",
                "process_instance_id={}\n",
                "purge_subject_sha256={}\n",
                "purge_generation={}\n",
                "storage_evidence_sha256={}\n",
                "before_inventory_count={}\n",
                "before_inventory_sha256={}\n",
                "after_inventory_count={}\n",
                "after_inventory_sha256={}\n",
                "removed_count={}\n",
                "runner_build_id={}\n",
                "completed_at_ms={}\n"
            ),
            RUNNER_PURGE_ACK_DOMAIN,
            self.version,
            self.audience,
            self.request_id,
            self.command_id,
            self.command_sha256,
            self.target_volume_id,
            self.target_key_fingerprint,
            self.enrollment_epoch,
            self.process_instance_id,
            self.purge_subject_sha256,
            self.purge_generation,
            self.storage_evidence_sha256,
            self.before_inventory_count,
            self.before_inventory_sha256,
            self.after_inventory_count,
            self.after_inventory_sha256,
            self.removed_count,
            self.runner_build_id,
            self.completed_at_ms,
        )
        .into_bytes())
    }

    /// Internal replay identity for the exact signed acknowledgement.
    pub fn ack_sha256(&self) -> RunnerVolumePurgeResult<String> {
        require_base64url(&self.signature, 64)?;
        let mut digest = Sha256::new();
        digest.update(self.canonical_unsigned_bytes()?);
        digest.update(b"signature=");
        digest.update(self.signature.as_bytes());
        Ok(hex::encode(digest.finalize()))
    }
}

pub fn sign_runner_purge_ack(
    signing_key: &Ed25519SigningKey,
    input: NewRunnerPurgeAck,
) -> RunnerVolumePurgeResult<RunnerPurgeAck> {
    let mut ack = RunnerPurgeAck::from_unsigned(input);
    ack.signature = encode_base64url(
        &signing_key
            .sign(&ack.canonical_unsigned_bytes()?)
            .to_bytes(),
    );
    Ok(ack)
}

pub fn verify_runner_purge_command(
    command: &RunnerPurgeCommand,
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<()> {
    command.validate_unsigned()?;
    verify_ed25519_signature(
        public_key_base64url,
        &command.canonical_unsigned_bytes()?,
        &command.signature,
    )
}

pub fn verify_runner_purge_ack(
    ack: &RunnerPurgeAck,
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<()> {
    ack.validate_unsigned()?;
    verify_ed25519_signature(
        public_key_base64url,
        &ack.canonical_unsigned_bytes()?,
        &ack.signature,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerVolumeStorageAttestation {
    pub attestation_id: String,
    pub volume_id: String,
    pub volume_key_fingerprint: String,
    pub resource_fingerprint: String,
    pub enrollment_epoch: i64,
    pub enrollment_generation: i64,
    pub process_instance_id: String,
    pub predecessor_attestation_generation: i64,
    pub predecessor_attestation_sha256: String,
    pub required_tombstone_generation: i64,
    pub reconciled_tombstone_generation: i64,
    pub storage_evidence_version: i64,
    pub subject_storage_layout_version: i64,
    pub root_device_id: String,
    pub root_link_count: i64,
    pub root_entry_count: i64,
    pub root_file_bytes: String,
    pub root_sha256: String,
    pub subject_storage_subject_count: i64,
    pub subject_storage_subject_set_sha256: String,
    pub subject_storage_scope_count: i64,
    pub subject_storage_complete_root_entry_count: i64,
    pub subject_storage_complete_root_file_bytes: String,
    pub subject_storage_complete_root_sha256: String,
    pub locator_count: i64,
    pub resident_locator_count: i64,
    pub locator_set_sha256: String,
    pub legacy_inventory_version: i64,
    pub legacy_artifact_count: i64,
    pub legacy_artifact_bytes: String,
    pub legacy_artifact_set_sha256: String,
    pub unclassified_root_count: i64,
    pub runner_build_id: String,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeStorageAttestation {
    pub version: i64,
    pub audience: String,
    pub attestation_id: String,
    pub volume_id: String,
    pub volume_key_fingerprint: String,
    pub resource_fingerprint: String,
    pub enrollment_epoch: i64,
    pub enrollment_generation: i64,
    pub process_instance_id: String,
    pub predecessor_attestation_generation: i64,
    pub predecessor_attestation_sha256: String,
    pub required_tombstone_generation: i64,
    pub reconciled_tombstone_generation: i64,
    pub storage_evidence_version: i64,
    pub subject_storage_layout_version: i64,
    pub root_device_id: String,
    pub root_link_count: i64,
    pub root_entry_count: i64,
    pub root_file_bytes: String,
    pub root_sha256: String,
    pub subject_storage_subject_count: i64,
    pub subject_storage_subject_set_sha256: String,
    pub subject_storage_scope_count: i64,
    pub subject_storage_complete_root_entry_count: i64,
    pub subject_storage_complete_root_file_bytes: String,
    pub subject_storage_complete_root_sha256: String,
    pub locator_count: i64,
    pub resident_locator_count: i64,
    pub locator_set_sha256: String,
    pub legacy_inventory_version: i64,
    pub legacy_artifact_count: i64,
    pub legacy_artifact_bytes: String,
    pub legacy_artifact_set_sha256: String,
    pub unclassified_root_count: i64,
    pub runner_build_id: String,
    pub observed_at_ms: i64,
    pub signature: String,
}

impl RunnerVolumeStorageAttestation {
    fn from_unsigned(input: NewRunnerVolumeStorageAttestation) -> Self {
        Self {
            version: RUNNER_STORAGE_ATTESTATION_VERSION,
            audience: RUNNER_STORAGE_ATTESTATION_AUDIENCE.to_string(),
            attestation_id: input.attestation_id,
            volume_id: input.volume_id,
            volume_key_fingerprint: input.volume_key_fingerprint,
            resource_fingerprint: input.resource_fingerprint,
            enrollment_epoch: input.enrollment_epoch,
            enrollment_generation: input.enrollment_generation,
            process_instance_id: input.process_instance_id,
            predecessor_attestation_generation: input.predecessor_attestation_generation,
            predecessor_attestation_sha256: input.predecessor_attestation_sha256,
            required_tombstone_generation: input.required_tombstone_generation,
            reconciled_tombstone_generation: input.reconciled_tombstone_generation,
            storage_evidence_version: input.storage_evidence_version,
            subject_storage_layout_version: input.subject_storage_layout_version,
            root_device_id: input.root_device_id,
            root_link_count: input.root_link_count,
            root_entry_count: input.root_entry_count,
            root_file_bytes: input.root_file_bytes,
            root_sha256: input.root_sha256,
            subject_storage_subject_count: input.subject_storage_subject_count,
            subject_storage_subject_set_sha256: input.subject_storage_subject_set_sha256,
            subject_storage_scope_count: input.subject_storage_scope_count,
            subject_storage_complete_root_entry_count: input
                .subject_storage_complete_root_entry_count,
            subject_storage_complete_root_file_bytes: input
                .subject_storage_complete_root_file_bytes,
            subject_storage_complete_root_sha256: input.subject_storage_complete_root_sha256,
            locator_count: input.locator_count,
            resident_locator_count: input.resident_locator_count,
            locator_set_sha256: input.locator_set_sha256,
            legacy_inventory_version: input.legacy_inventory_version,
            legacy_artifact_count: input.legacy_artifact_count,
            legacy_artifact_bytes: input.legacy_artifact_bytes,
            legacy_artifact_set_sha256: input.legacy_artifact_set_sha256,
            unclassified_root_count: input.unclassified_root_count,
            runner_build_id: input.runner_build_id,
            observed_at_ms: input.observed_at_ms,
            signature: String::new(),
        }
    }

    fn validate_unsigned(&self) -> RunnerVolumePurgeResult<()> {
        if self.version != RUNNER_STORAGE_ATTESTATION_VERSION
            || self.audience != RUNNER_STORAGE_ATTESTATION_AUDIENCE
            || self.storage_evidence_version != RUNNER_PURGE_STORAGE_EVIDENCE_VERSION
            || self.subject_storage_layout_version != RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION
            || self.legacy_inventory_version != RUNNER_LEGACY_INVENTORY_VERSION
            || self.required_tombstone_generation != self.reconciled_tombstone_generation
            || self.root_link_count < 1
            || self.subject_storage_scope_count != self.resident_locator_count
            || self.resident_locator_count > self.locator_count
            || self.subject_storage_complete_root_entry_count > self.root_entry_count
            || parse_canonical_u64(&self.subject_storage_complete_root_file_bytes)?
                > parse_canonical_u64(&self.root_file_bytes)?
            || self.legacy_artifact_count != 0
            || self.legacy_artifact_bytes != "0"
            || self.legacy_artifact_set_sha256 != EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256
            || self.unclassified_root_count != 0
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.attestation_id)?;
        require_base64url(&self.volume_id, 32)?;
        require_sha256(&self.volume_key_fingerprint)?;
        require_sha256(&self.resource_fingerprint)?;
        require_base64url(&self.process_instance_id, 32)?;
        for value in [
            self.enrollment_epoch,
            self.enrollment_generation,
            self.predecessor_attestation_generation,
            self.required_tombstone_generation,
            self.reconciled_tombstone_generation,
            self.root_link_count,
            self.root_entry_count,
            self.subject_storage_subject_count,
            self.subject_storage_scope_count,
            self.subject_storage_complete_root_entry_count,
            self.locator_count,
            self.resident_locator_count,
            self.legacy_artifact_count,
            self.unclassified_root_count,
            self.observed_at_ms,
        ] {
            validate_nonnegative_safe_integer(value)?;
        }
        if self.enrollment_epoch == 0 || self.enrollment_generation == 0 {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_sha256(&self.predecessor_attestation_sha256)?;
        if (self.predecessor_attestation_generation == 0)
            != (self.predecessor_attestation_sha256 == GENESIS_RUNNER_STORAGE_ATTESTATION_SHA256)
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.root_device_id)?;
        parse_canonical_u64(&self.root_file_bytes)?;
        parse_canonical_u64(&self.subject_storage_complete_root_file_bytes)?;
        for sha256 in [
            &self.root_sha256,
            &self.subject_storage_subject_set_sha256,
            &self.subject_storage_complete_root_sha256,
            &self.locator_set_sha256,
            &self.legacy_artifact_set_sha256,
        ] {
            require_sha256(sha256)?;
        }
        if !runner_build_id_is_canonical(&self.runner_build_id) {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        Ok(())
    }

    /// Frozen Node/Rust byte contract. The first domain line and final newline
    /// are both covered by the runner's Ed25519 signature.
    pub fn canonical_unsigned_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate_unsigned()?;
        Ok(format!(
            concat!(
                "{}\n",
                "version={}\n",
                "audience={}\n",
                "attestation_id={}\n",
                "volume_id={}\n",
                "volume_key_fingerprint={}\n",
                "resource_fingerprint={}\n",
                "enrollment_epoch={}\n",
                "enrollment_generation={}\n",
                "process_instance_id={}\n",
                "predecessor_attestation_generation={}\n",
                "predecessor_attestation_sha256={}\n",
                "required_tombstone_generation={}\n",
                "reconciled_tombstone_generation={}\n",
                "storage_evidence_version={}\n",
                "subject_storage_layout_version={}\n",
                "root_device_id={}\n",
                "root_link_count={}\n",
                "root_entry_count={}\n",
                "root_file_bytes={}\n",
                "root_sha256={}\n",
                "subject_storage_subject_count={}\n",
                "subject_storage_subject_set_sha256={}\n",
                "subject_storage_scope_count={}\n",
                "subject_storage_complete_root_entry_count={}\n",
                "subject_storage_complete_root_file_bytes={}\n",
                "subject_storage_complete_root_sha256={}\n",
                "locator_count={}\n",
                "resident_locator_count={}\n",
                "locator_set_sha256={}\n",
                "legacy_inventory_version={}\n",
                "legacy_artifact_count={}\n",
                "legacy_artifact_bytes={}\n",
                "legacy_artifact_set_sha256={}\n",
                "unclassified_root_count={}\n",
                "runner_build_id={}\n",
                "observed_at_ms={}\n"
            ),
            RUNNER_STORAGE_ATTESTATION_DOMAIN,
            self.version,
            self.audience,
            self.attestation_id,
            self.volume_id,
            self.volume_key_fingerprint,
            self.resource_fingerprint,
            self.enrollment_epoch,
            self.enrollment_generation,
            self.process_instance_id,
            self.predecessor_attestation_generation,
            self.predecessor_attestation_sha256,
            self.required_tombstone_generation,
            self.reconciled_tombstone_generation,
            self.storage_evidence_version,
            self.subject_storage_layout_version,
            self.root_device_id,
            self.root_link_count,
            self.root_entry_count,
            self.root_file_bytes,
            self.root_sha256,
            self.subject_storage_subject_count,
            self.subject_storage_subject_set_sha256,
            self.subject_storage_scope_count,
            self.subject_storage_complete_root_entry_count,
            self.subject_storage_complete_root_file_bytes,
            self.subject_storage_complete_root_sha256,
            self.locator_count,
            self.resident_locator_count,
            self.locator_set_sha256,
            self.legacy_inventory_version,
            self.legacy_artifact_count,
            self.legacy_artifact_bytes,
            self.legacy_artifact_set_sha256,
            self.unclassified_root_count,
            self.runner_build_id,
            self.observed_at_ms,
        )
        .into_bytes())
    }

    pub fn canonical_unsigned_sha256(&self) -> RunnerVolumePurgeResult<String> {
        Ok(hex::encode(Sha256::digest(
            self.canonical_unsigned_bytes()?,
        )))
    }

    pub fn attestation_sha256(&self) -> RunnerVolumePurgeResult<String> {
        require_base64url(&self.signature, 64)?;
        let mut digest = Sha256::new();
        digest.update(self.canonical_unsigned_bytes()?);
        digest.update(b"signature=");
        digest.update(self.signature.as_bytes());
        digest.update(b"\n");
        Ok(hex::encode(digest.finalize()))
    }
}

pub fn sign_runner_volume_storage_attestation(
    signing_key: &Ed25519SigningKey,
    input: NewRunnerVolumeStorageAttestation,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestation> {
    let mut attestation = RunnerVolumeStorageAttestation::from_unsigned(input);
    attestation.signature = encode_base64url(
        &signing_key
            .sign(&attestation.canonical_unsigned_bytes()?)
            .to_bytes(),
    );
    Ok(attestation)
}

pub fn verify_runner_volume_storage_attestation(
    attestation: &RunnerVolumeStorageAttestation,
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<()> {
    attestation.validate_unsigned()?;
    verify_ed25519_signature(
        public_key_base64url,
        &attestation.canonical_unsigned_bytes()?,
        &attestation.signature,
    )
}

fn verify_ed25519_signature(
    public_key_base64url: &str,
    message: &[u8],
    signature_base64url: &str,
) -> RunnerVolumePurgeResult<()> {
    let public_key = decode_base64url_exact(public_key_base64url, 32)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    let verifying_key = Ed25519VerifyingKey::from_bytes(&public_key)
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    let signature = decode_base64url_exact(signature_base64url, 64)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    verifying_key
        .verify(message, &Ed25519Signature::from_bytes(&signature))
        .map_err(|_| RunnerVolumePurgeError::Unauthorized)
}

pub fn runner_volume_id_from_public_key(
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<String> {
    let public_key = decode_base64url_exact(public_key_base64url, 32)?;
    let mut digest = Sha256::new();
    digest.update(RUNNER_VOLUME_ID_DOMAIN);
    digest.update(public_key);
    Ok(encode_base64url(&digest.finalize()))
}

pub fn runner_volume_key_fingerprint(
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<String> {
    Ok(hex::encode(Sha256::digest(decode_base64url_exact(
        public_key_base64url,
        32,
    )?)))
}

pub fn runner_purge_subject_sha256(purge_subject: &str) -> RunnerVolumePurgeResult<String> {
    Ok(hex::encode(Sha256::digest(decode_base64url_exact(
        purge_subject,
        32,
    )?)))
}

fn encode_base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_base64url_exact(value: &str, expected_bytes: usize) -> RunnerVolumePurgeResult<Vec<u8>> {
    if value.is_empty() || !value.bytes().all(is_base64url_byte) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    if decoded.len() != expected_bytes || encode_base64url(&decoded) != value {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(decoded)
}

fn is_base64url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn require_base64url(value: &str, expected_bytes: usize) -> RunnerVolumePurgeResult<()> {
    decode_base64url_exact(value, expected_bytes).map(|_| ())
}

fn require_sha256(value: &str) -> RunnerVolumePurgeResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(RunnerVolumePurgeError::InvalidRequest)
    }
}

fn validate_legacy_inventory_authority_binding(
    generation: i64,
    sha256: &str,
) -> RunnerVolumePurgeResult<()> {
    validate_nonnegative_safe_integer(generation)?;
    require_sha256(sha256)?;
    if (generation == 0) != (sha256 == ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn validate_nonnegative_safe_integer(value: i64) -> RunnerVolumePurgeResult<()> {
    if (0..=MAX_JAVASCRIPT_SAFE_INTEGER).contains(&value) {
        Ok(())
    } else {
        Err(RunnerVolumePurgeError::InvalidRequest)
    }
}

fn parse_canonical_u64(value: &str) -> RunnerVolumePurgeResult<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    value
        .parse::<u64>()
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)
}

fn validate_inventory_evidence(
    inventory: &RunnerPurgeInventoryEvidence,
    empty_sha256: &str,
) -> RunnerVolumePurgeResult<()> {
    validate_nonnegative_safe_integer(inventory.entry_count)?;
    let file_bytes = parse_canonical_u64(&inventory.file_bytes)?;
    require_sha256(&inventory.sha256)?;
    if inventory.entry_count == 0 {
        if file_bytes != 0 || inventory.sha256 != empty_sha256 {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
    } else if inventory.sha256 == empty_sha256 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn is_empty_inventory(inventory: &RunnerPurgeInventoryEvidence, empty_sha256: &str) -> bool {
    inventory.entry_count == 0 && inventory.file_bytes == "0" && inventory.sha256 == empty_sha256
}

fn validate_root_snapshot(
    snapshot: &RunnerPurgeRootSnapshotEvidence,
) -> RunnerVolumePurgeResult<()> {
    if snapshot.link_count <= 0 || snapshot.link_count > MAX_JAVASCRIPT_SAFE_INTEGER {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    validate_inventory_evidence(
        &RunnerPurgeInventoryEvidence {
            entry_count: snapshot.entry_count,
            file_bytes: snapshot.file_bytes.clone(),
            sha256: snapshot.sha256.clone(),
        },
        EMPTY_RUNNER_INVENTORY_SHA256,
    )
}

fn validate_subject_storage_snapshot(
    snapshot: &RunnerPurgeSubjectStorageSnapshotEvidence,
) -> RunnerVolumePurgeResult<()> {
    if !matches!(snapshot.residency.as_str(), "never_resident" | "resident") {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    for inventory in [
        &snapshot.subject_tree,
        &snapshot.ownership,
        &snapshot.target,
        &snapshot.complete_root,
    ] {
        validate_inventory_evidence(inventory, EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256)?;
    }
    let target_entry_count = snapshot
        .subject_tree
        .entry_count
        .checked_add(snapshot.ownership.entry_count)
        .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
    let target_file_bytes = parse_canonical_u64(&snapshot.subject_tree.file_bytes)?
        .checked_add(parse_canonical_u64(&snapshot.ownership.file_bytes)?)
        .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
    if snapshot.target.entry_count != target_entry_count
        || parse_canonical_u64(&snapshot.target.file_bytes)? != target_file_bytes
        || snapshot.complete_root.entry_count < snapshot.target.entry_count
        || parse_canonical_u64(&snapshot.complete_root.file_bytes)? < target_file_bytes
        || (snapshot.residency == "never_resident"
            && !is_empty_inventory(
                &snapshot.target,
                EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256,
            ))
        || (snapshot.residency == "resident" && snapshot.target.entry_count == 0)
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn validate_legacy_root_snapshot(
    snapshot: &RunnerPurgeLegacyRootSnapshotEvidence,
) -> RunnerVolumePurgeResult<()> {
    validate_nonnegative_safe_integer(snapshot.artifact_count)?;
    validate_nonnegative_safe_integer(snapshot.unclassified_root_count)?;
    let artifact_bytes = parse_canonical_u64(&snapshot.artifact_bytes)?;
    require_sha256(&snapshot.artifact_set_sha256)?;
    if snapshot.unclassified_root_count > snapshot.artifact_count
        || (snapshot.artifact_count == 0
            && (artifact_bytes != 0
                || snapshot.unclassified_root_count != 0
                || snapshot.artifact_set_sha256 != EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256))
        || (snapshot.artifact_count > 0
            && snapshot.artifact_set_sha256 == EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256)
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn is_empty_legacy_root_snapshot(snapshot: &RunnerPurgeLegacyRootSnapshotEvidence) -> bool {
    snapshot.artifact_count == 0
        && snapshot.artifact_bytes == "0"
        && snapshot.artifact_set_sha256 == EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256
        && snapshot.unclassified_root_count == 0
}

fn validate_snapshot_subsets(
    root: &RunnerPurgeRootSnapshotEvidence,
    subject_root: &RunnerPurgeInventoryEvidence,
    legacy_root: &RunnerPurgeLegacyRootSnapshotEvidence,
) -> RunnerVolumePurgeResult<()> {
    let classified_count = subject_root
        .entry_count
        .checked_add(legacy_root.artifact_count)
        .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
    let classified_bytes = parse_canonical_u64(&subject_root.file_bytes)?
        .checked_add(parse_canonical_u64(&legacy_root.artifact_bytes)?)
        .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
    if classified_count > root.entry_count
        || classified_bytes > parse_canonical_u64(&root.file_bytes)?
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn validate_inventory_subset(
    inventory: &RunnerPurgeInventoryEvidence,
    superset_count: i64,
    superset_bytes: &str,
) -> RunnerVolumePurgeResult<()> {
    if inventory.entry_count > superset_count
        || parse_canonical_u64(&inventory.file_bytes)? > parse_canonical_u64(superset_bytes)?
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn require_runner_identifier(value: &str) -> RunnerVolumePurgeResult<()> {
    let mut bytes = value.bytes();
    if value.len() > MAX_RUNNER_IDENTIFIER_BYTES
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'+' | b'-')
        })
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn require_nonempty_text(value: &str, maximum_len: usize) -> RunnerVolumePurgeResult<()> {
    if value.is_empty()
        || value.len() > maximum_len
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerVolumeWriteDisposition {
    Applied,
    Replay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerVolumeAdmissionGrant {
    pub grant_id: String,
    /// High-entropy one-time bearer value. Only its SHA-256 is persisted.
    pub token: String,
    pub expected_worker_id: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub authorization_ref: String,
    pub created_by: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeAdmissionGrant {
    pub grant_id: String,
    pub token_sha256: String,
    pub expected_worker_id: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub authorization_ref: String,
    pub created_by: String,
    pub issued_fleet_generation: i64,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub consumed_volume_id: Option<String>,
    pub consumed_at_ms: Option<i64>,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerVolumeEnrollmentProof {
    pub admission_grant_id: String,
    pub volume_id: String,
    pub worker_id: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub enrollment_epoch: i64,
    pub public_key_base64url: String,
    pub key_fingerprint: String,
    pub legacy_artifact_count: i64,
    pub requested_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeEnrollmentProof {
    pub admission_grant_id: String,
    pub volume_id: String,
    pub worker_id: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub enrollment_epoch: i64,
    pub public_key_base64url: String,
    pub key_fingerprint: String,
    pub legacy_artifact_count: i64,
    pub requested_at_ms: i64,
    pub signature: String,
}

impl RunnerVolumeEnrollmentProof {
    fn from_unsigned(input: NewRunnerVolumeEnrollmentProof) -> Self {
        Self {
            admission_grant_id: input.admission_grant_id,
            volume_id: input.volume_id,
            worker_id: input.worker_id,
            provider: input.provider,
            provider_resource_id: input.provider_resource_id,
            resource_fingerprint: input.resource_fingerprint,
            enrollment_epoch: input.enrollment_epoch,
            public_key_base64url: input.public_key_base64url,
            key_fingerprint: input.key_fingerprint,
            legacy_artifact_count: input.legacy_artifact_count,
            requested_at_ms: input.requested_at_ms,
            signature: String::new(),
        }
    }

    fn validate_unsigned(&self) -> RunnerVolumePurgeResult<()> {
        require_runner_identifier(&self.admission_grant_id)?;
        require_base64url(&self.volume_id, 32)?;
        require_runner_identifier(&self.worker_id)?;
        require_runner_identifier(&self.provider)?;
        require_nonempty_text(&self.provider_resource_id, 512)?;
        require_sha256(&self.resource_fingerprint)?;
        if self.enrollment_epoch != 1 || self.legacy_artifact_count < 0 || self.requested_at_ms < 0
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_base64url(&self.public_key_base64url, 32)?;
        require_sha256(&self.key_fingerprint)?;
        if runner_volume_id_from_public_key(&self.public_key_base64url)? != self.volume_id
            || runner_volume_key_fingerprint(&self.public_key_base64url)? != self.key_fingerprint
        {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        Ok(())
    }

    pub fn canonical_unsigned_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate_unsigned()?;
        Ok(format!(
            concat!(
                "{}\n",
                "admission_grant_id={}\n",
                "volume_id={}\n",
                "worker_id={}\n",
                "provider={}\n",
                "provider_resource_id={}\n",
                "resource_fingerprint={}\n",
                "enrollment_epoch={}\n",
                "public_key_base64url={}\n",
                "key_fingerprint={}\n",
                "legacy_artifact_count={}\n",
                "requested_at_ms={}\n"
            ),
            RUNNER_VOLUME_ENROLLMENT_DOMAIN,
            self.admission_grant_id,
            self.volume_id,
            self.worker_id,
            self.provider,
            self.provider_resource_id,
            self.resource_fingerprint,
            self.enrollment_epoch,
            self.public_key_base64url,
            self.key_fingerprint,
            self.legacy_artifact_count,
            self.requested_at_ms,
        )
        .into_bytes())
    }
}

pub fn sign_runner_volume_enrollment_proof(
    signing_key: &Ed25519SigningKey,
    input: NewRunnerVolumeEnrollmentProof,
) -> RunnerVolumePurgeResult<RunnerVolumeEnrollmentProof> {
    let mut proof = RunnerVolumeEnrollmentProof::from_unsigned(input);
    proof.signature = encode_base64url(
        &signing_key
            .sign(&proof.canonical_unsigned_bytes()?)
            .to_bytes(),
    );
    Ok(proof)
}

pub fn verify_runner_volume_enrollment_proof(
    proof: &RunnerVolumeEnrollmentProof,
) -> RunnerVolumePurgeResult<()> {
    proof.validate_unsigned()?;
    verify_ed25519_signature(
        &proof.public_key_base64url,
        &proof.canonical_unsigned_bytes()?,
        &proof.signature,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerVolumeAuthorityProof {
    pub operation: String,
    pub request_id: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub issued_at_ms: i64,
    pub payload_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeAuthorityProof {
    pub version: i64,
    pub audience: String,
    pub operation: String,
    pub request_id: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub issued_at_ms: i64,
    pub payload_sha256: String,
    pub signature: String,
}

impl RunnerVolumeAuthorityProof {
    fn from_unsigned(input: NewRunnerVolumeAuthorityProof) -> Self {
        Self {
            version: 1,
            audience: RUNNER_VOLUME_AUTHORITY_AUDIENCE.to_string(),
            operation: input.operation,
            request_id: input.request_id,
            volume_id: input.volume_id,
            enrollment_epoch: input.enrollment_epoch,
            process_instance_id: input.process_instance_id,
            issued_at_ms: input.issued_at_ms,
            payload_sha256: input.payload_sha256,
            signature: String::new(),
        }
    }

    fn validate_unsigned(&self) -> RunnerVolumePurgeResult<()> {
        if self.version != 1
            || self.audience != RUNNER_VOLUME_AUTHORITY_AUDIENCE
            || !matches!(
                self.operation.as_str(),
                "instance_claim"
                    | "instance_heartbeat"
                    | "residency_bind"
                    | "purge_poll"
                    | "storage_attestation"
                    | "execution_lease_claim"
            )
            || self.enrollment_epoch <= 0
            || self.issued_at_ms < 0
        {
            return Err(RunnerVolumePurgeError::InvalidRequest);
        }
        require_runner_identifier(&self.request_id)?;
        require_base64url(&self.volume_id, 32)?;
        require_base64url(&self.process_instance_id, 32)?;
        require_sha256(&self.payload_sha256)
    }

    pub fn canonical_unsigned_bytes(&self) -> RunnerVolumePurgeResult<Vec<u8>> {
        self.validate_unsigned()?;
        Ok(format!(
            concat!(
                "{}\n",
                "version={}\n",
                "audience={}\n",
                "operation={}\n",
                "request_id={}\n",
                "volume_id={}\n",
                "enrollment_epoch={}\n",
                "process_instance_id={}\n",
                "issued_at_ms={}\n",
                "payload_sha256={}\n"
            ),
            RUNNER_VOLUME_AUTHORITY_DOMAIN,
            self.version,
            self.audience,
            self.operation,
            self.request_id,
            self.volume_id,
            self.enrollment_epoch,
            self.process_instance_id,
            self.issued_at_ms,
            self.payload_sha256,
        )
        .into_bytes())
    }
}

pub fn sign_runner_volume_authority_proof(
    signing_key: &Ed25519SigningKey,
    input: NewRunnerVolumeAuthorityProof,
) -> RunnerVolumePurgeResult<RunnerVolumeAuthorityProof> {
    let mut proof = RunnerVolumeAuthorityProof::from_unsigned(input);
    proof.signature = encode_base64url(
        &signing_key
            .sign(&proof.canonical_unsigned_bytes()?)
            .to_bytes(),
    );
    Ok(proof)
}

pub fn verify_runner_volume_authority_signature(
    proof: &RunnerVolumeAuthorityProof,
    public_key_base64url: &str,
) -> RunnerVolumePurgeResult<()> {
    proof.validate_unsigned()?;
    verify_ed25519_signature(
        public_key_base64url,
        &proof.canonical_unsigned_bytes()?,
        &proof.signature,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollRunnerVolumeRequest {
    pub grant_token: String,
    pub proof: RunnerVolumeEnrollmentProof,
    pub enrolled_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeRecord {
    pub volume_id: String,
    pub worker_id: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub current_epoch: i64,
    pub enrollment_generation: i64,
    pub required_tombstone_generation: i64,
    pub reconciled_tombstone_generation: i64,
    pub status: String,
    pub active_instance_id: Option<String>,
    pub instance_lease_expires_at_ms: Option<i64>,
    pub legacy_artifact_count: i64,
    pub admission_grant_id: String,
    pub enrolled_at_ms: i64,
    pub last_seen_at_ms: i64,
    pub updated_at_ms: i64,
    pub public_key_base64url: String,
    pub key_fingerprint: String,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeInstanceLease {
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub runtime_grant_id: Option<String>,
    pub runtime_sha256: Option<String>,
    pub lease_expires_at_ms: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Copy)]
pub struct RunnerVolumeInstanceLeaseRequest<'a> {
    pub volume_id: &'a str,
    pub enrollment_epoch: i64,
    pub process_instance_id: &'a str,
    pub now_ms: i64,
    pub lease_expires_at_ms: i64,
}

/// Exact cloud process runtime measured by the runner and approved separately
/// by deployment. This is cooperative software attestation; authority comes
/// from the one-time server grant and the immutable server-side binding, not
/// from values echoed in a lease response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerProcessRuntimeAttestation {
    pub runner_image_sha256: String,
    pub runner_build_id: String,
    pub platform: String,
    pub architecture: String,
    pub automation_bundle_sha256: String,
    pub playwright_version: String,
    pub chromium_revision: String,
    pub chromium_executable_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRunnerProcessRuntimeGrant {
    pub grant_id: String,
    /// High-entropy one-time deployment bearer. Only its SHA-256 is stored.
    pub token: String,
    pub expected_worker_id: String,
    pub runtime: RunnerProcessRuntimeAttestation,
    pub authorization_ref: String,
    pub created_by: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerProcessRuntimeGrant {
    pub grant_id: String,
    pub token_sha256: String,
    pub expected_worker_id: String,
    pub runtime_sha256: String,
    pub runtime: RunnerProcessRuntimeAttestation,
    pub authorization_ref: String,
    pub created_by: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeRunnerProcessRuntimeGrantRequest {
    pub grant_id: String,
    pub reason: String,
    pub authorization_ref: String,
    pub revoked_by: String,
    pub revoked_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerProcessRuntimeGrantRevocation {
    pub grant_id: String,
    pub reason: String,
    pub authorization_ref: String,
    pub revoked_by: String,
    pub revoked_at_ms: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerProcessRuntimeGrantClaim {
    pub grant_id: String,
    pub grant_token: String,
    pub runtime: RunnerProcessRuntimeAttestation,
}

/// Runtime identity loaded from immutable server records. Callers must use
/// this value (or reload it transactionally), never request or lease JSON, for
/// ATS Phase A and other irreversible cloud authority checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedRunnerProcessRuntimeAttestation {
    pub runtime_grant_id: String,
    pub worker_id: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub runtime_sha256: String,
    pub runtime: RunnerProcessRuntimeAttestation,
    pub bound_at_ms: i64,
}

fn validate_runner_process_runtime(
    runtime: &RunnerProcessRuntimeAttestation,
) -> RunnerVolumePurgeResult<()> {
    require_sha256(&runtime.runner_image_sha256)?;
    require_runner_identifier(&runtime.runner_build_id)?;
    if !matches!(runtime.platform.as_str(), "linux" | "macos" | "windows")
        || !matches!(runtime.architecture.as_str(), "arm64" | "x86_64")
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    require_sha256(&runtime.automation_bundle_sha256)?;
    require_nonempty_text(&runtime.playwright_version, 80)?;
    require_nonempty_text(&runtime.chromium_revision, 80)?;
    require_sha256(&runtime.chromium_executable_sha256)?;
    Ok(())
}

pub fn runner_process_runtime_sha256(
    runtime: &RunnerProcessRuntimeAttestation,
) -> RunnerVolumePurgeResult<String> {
    validate_runner_process_runtime(runtime)?;
    Ok(hex::encode(Sha256::digest(
        format!(
            concat!(
                "{}\n",
                "runner_image_sha256={}\n",
                "runner_build_id={}\n",
                "platform={}\n",
                "architecture={}\n",
                "automation_bundle_sha256={}\n",
                "playwright_version={}\n",
                "chromium_revision={}\n",
                "chromium_executable_sha256={}\n"
            ),
            RUNNER_PROCESS_RUNTIME_DOMAIN,
            runtime.runner_image_sha256,
            runtime.runner_build_id,
            runtime.platform,
            runtime.architecture,
            runtime.automation_bundle_sha256,
            runtime.playwright_version,
            runtime.chromium_revision,
            runtime.chromium_executable_sha256,
        )
        .as_bytes(),
    )))
}

fn validate_admission_grant(
    input: &NewRunnerVolumeAdmissionGrant,
) -> RunnerVolumePurgeResult<String> {
    require_runner_identifier(&input.grant_id)?;
    let token = decode_base64url_exact(&input.token, 32)?;
    require_runner_identifier(&input.expected_worker_id)?;
    require_runner_identifier(&input.provider)?;
    require_nonempty_text(&input.provider_resource_id, 512)?;
    require_sha256(&input.resource_fingerprint)?;
    require_nonempty_text(&input.authorization_ref, 1_024)?;
    require_nonempty_text(&input.created_by, 240)?;
    if input.created_at_ms < 0 || input.expires_at_ms <= input.created_at_ms {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(hex::encode(Sha256::digest(token)))
}

fn validate_enrollment_request(input: &EnrollRunnerVolumeRequest) -> RunnerVolumePurgeResult<()> {
    verify_runner_volume_enrollment_proof(&input.proof)?;
    require_base64url(&input.grant_token, 32)?;
    if input.enrolled_at_ms < input.proof.requested_at_ms || input.enrolled_at_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn admission_grant_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RunnerVolumeAdmissionGrant> {
    Ok(RunnerVolumeAdmissionGrant {
        grant_id: row.get(0)?,
        token_sha256: row.get(1)?,
        expected_worker_id: row.get(2)?,
        provider: row.get(3)?,
        provider_resource_id: row.get(4)?,
        resource_fingerprint: row.get(5)?,
        authorization_ref: row.get(6)?,
        created_by: row.get(7)?,
        issued_fleet_generation: row.get(8)?,
        expires_at_ms: row.get(9)?,
        created_at_ms: row.get(10)?,
        consumed_volume_id: row.get(11)?,
        consumed_at_ms: row.get(12)?,
        disposition: RunnerVolumeWriteDisposition::Replay,
    })
}

fn admission_grant_from_pg_row(row: postgres::Row) -> RunnerVolumeAdmissionGrant {
    RunnerVolumeAdmissionGrant {
        grant_id: row.get(0),
        token_sha256: row.get(1),
        expected_worker_id: row.get(2),
        provider: row.get(3),
        provider_resource_id: row.get(4),
        resource_fingerprint: row.get(5),
        authorization_ref: row.get(6),
        created_by: row.get(7),
        issued_fleet_generation: row.get(8),
        expires_at_ms: row.get(9),
        created_at_ms: row.get(10),
        consumed_volume_id: row.get(11),
        consumed_at_ms: row.get(12),
        disposition: RunnerVolumeWriteDisposition::Replay,
    }
}

const ADMISSION_GRANT_COLUMNS: &str =
    "grant_id, token_sha256, expected_worker_id, provider, provider_resource_id, \
     resource_fingerprint, authorization_ref, created_by, issued_fleet_generation, \
     expires_at_ms, created_at_ms, consumed_volume_id, consumed_at_ms";

fn exact_admission_grant(
    stored: &RunnerVolumeAdmissionGrant,
    input: &NewRunnerVolumeAdmissionGrant,
    token_sha256: &str,
) -> bool {
    stored.grant_id == input.grant_id
        && stored.token_sha256 == token_sha256
        && stored.expected_worker_id == input.expected_worker_id
        && stored.provider == input.provider
        && stored.provider_resource_id == input.provider_resource_id
        && stored.resource_fingerprint == input.resource_fingerprint
        && stored.authorization_ref == input.authorization_ref
        && stored.created_by == input.created_by
        && stored.expires_at_ms == input.expires_at_ms
        && stored.created_at_ms == input.created_at_ms
}

pub fn create_runner_volume_admission_grant(
    pool: &DbPool,
    input: &NewRunnerVolumeAdmissionGrant,
) -> RunnerVolumePurgeResult<RunnerVolumeAdmissionGrant> {
    let token_sha256 = validate_admission_grant(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let fleet_generation: i64 = tx.query_row(
                "SELECT enrollment_generation FROM jobs_runner_volume_fleet_state \
                  WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            let select = format!(
                "SELECT {ADMISSION_GRANT_COLUMNS} \
                   FROM jobs_runner_volume_admission_grants WHERE grant_id = ?1"
            );
            if let Some(stored) = tx
                .query_row(
                    &select,
                    params![input.grant_id],
                    admission_grant_from_sqlite_row,
                )
                .optional()?
            {
                if !exact_admission_grant(&stored, input, &token_sha256) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            tx.execute(
                "INSERT INTO jobs_runner_volume_admission_grants ( \
                    grant_id, token_sha256, expected_worker_id, provider, \
                    provider_resource_id, resource_fingerprint, authorization_ref, \
                    created_by, issued_fleet_generation, expires_at_ms, created_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    input.grant_id,
                    token_sha256,
                    input.expected_worker_id,
                    input.provider,
                    input.provider_resource_id,
                    input.resource_fingerprint,
                    input.authorization_ref,
                    input.created_by,
                    fleet_generation,
                    input.expires_at_ms,
                    input.created_at_ms,
                ],
            )?;
            let mut created = tx.query_row(
                &select,
                params![input.grant_id],
                admission_grant_from_sqlite_row,
            )?;
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let fleet_generation: i64 = tx
                .query_one(
                    "SELECT enrollment_generation FROM jobs_runner_volume_fleet_state \
                      WHERE singleton_id = 1 FOR UPDATE",
                    &[],
                )?
                .get(0);
            let select = format!(
                "SELECT {ADMISSION_GRANT_COLUMNS} \
                   FROM jobs_runner_volume_admission_grants WHERE grant_id = $1 FOR UPDATE"
            );
            if let Some(row) = tx.query_opt(&select, &[&input.grant_id])? {
                let stored = admission_grant_from_pg_row(row);
                if !exact_admission_grant(&stored, input, &token_sha256) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            tx.execute(
                "INSERT INTO jobs_runner_volume_admission_grants ( \
                    grant_id, token_sha256, expected_worker_id, provider, \
                    provider_resource_id, resource_fingerprint, authorization_ref, \
                    created_by, issued_fleet_generation, expires_at_ms, created_at_ms \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &input.grant_id,
                    &token_sha256,
                    &input.expected_worker_id,
                    &input.provider,
                    &input.provider_resource_id,
                    &input.resource_fingerprint,
                    &input.authorization_ref,
                    &input.created_by,
                    &fleet_generation,
                    &input.expires_at_ms,
                    &input.created_at_ms,
                ],
            )?;
            let mut created =
                admission_grant_from_pg_row(tx.query_one(&select, &[&input.grant_id])?);
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
    })
}

const PROCESS_RUNTIME_GRANT_COLUMNS: &str =
    "grant_id, token_sha256, expected_worker_id, runtime_sha256, runner_image_sha256, \
     runner_build_id, platform, architecture, automation_bundle_sha256, \
     playwright_version, chromium_revision, chromium_executable_sha256, \
     authorization_ref, created_by, expires_at_ms, created_at_ms";

fn process_runtime_grant_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RunnerProcessRuntimeGrant> {
    Ok(RunnerProcessRuntimeGrant {
        grant_id: row.get(0)?,
        token_sha256: row.get(1)?,
        expected_worker_id: row.get(2)?,
        runtime_sha256: row.get(3)?,
        runtime: RunnerProcessRuntimeAttestation {
            runner_image_sha256: row.get(4)?,
            runner_build_id: row.get(5)?,
            platform: row.get(6)?,
            architecture: row.get(7)?,
            automation_bundle_sha256: row.get(8)?,
            playwright_version: row.get(9)?,
            chromium_revision: row.get(10)?,
            chromium_executable_sha256: row.get(11)?,
        },
        authorization_ref: row.get(12)?,
        created_by: row.get(13)?,
        expires_at_ms: row.get(14)?,
        created_at_ms: row.get(15)?,
        disposition: RunnerVolumeWriteDisposition::Replay,
    })
}

fn process_runtime_grant_from_pg_row(row: postgres::Row) -> RunnerProcessRuntimeGrant {
    RunnerProcessRuntimeGrant {
        grant_id: row.get(0),
        token_sha256: row.get(1),
        expected_worker_id: row.get(2),
        runtime_sha256: row.get(3),
        runtime: RunnerProcessRuntimeAttestation {
            runner_image_sha256: row.get(4),
            runner_build_id: row.get(5),
            platform: row.get(6),
            architecture: row.get(7),
            automation_bundle_sha256: row.get(8),
            playwright_version: row.get(9),
            chromium_revision: row.get(10),
            chromium_executable_sha256: row.get(11),
        },
        authorization_ref: row.get(12),
        created_by: row.get(13),
        expires_at_ms: row.get(14),
        created_at_ms: row.get(15),
        disposition: RunnerVolumeWriteDisposition::Replay,
    }
}

fn validate_new_runner_process_runtime_grant(
    input: &NewRunnerProcessRuntimeGrant,
) -> RunnerVolumePurgeResult<(String, String)> {
    require_runner_identifier(&input.grant_id)?;
    let token = decode_base64url_exact(&input.token, 32)?;
    require_runner_identifier(&input.expected_worker_id)?;
    let runtime_sha256 = runner_process_runtime_sha256(&input.runtime)?;
    require_nonempty_text(&input.authorization_ref, 1_024)?;
    require_nonempty_text(&input.created_by, 240)?;
    if input.created_at_ms < 0 || input.expires_at_ms <= input.created_at_ms {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok((hex::encode(Sha256::digest(token)), runtime_sha256))
}

fn exact_process_runtime_grant(
    stored: &RunnerProcessRuntimeGrant,
    input: &NewRunnerProcessRuntimeGrant,
    token_sha256: &str,
    runtime_sha256: &str,
) -> bool {
    stored.grant_id == input.grant_id
        && stored
            .token_sha256
            .as_bytes()
            .ct_eq(token_sha256.as_bytes())
            .unwrap_u8()
            == 1
        && stored.expected_worker_id == input.expected_worker_id
        && stored.runtime_sha256 == runtime_sha256
        && stored.runtime == input.runtime
        && stored.authorization_ref == input.authorization_ref
        && stored.created_by == input.created_by
        && stored.expires_at_ms == input.expires_at_ms
        && stored.created_at_ms == input.created_at_ms
}

/// Persist one immutable deployment approval for an exact cloud process
/// runtime. The raw bearer is returned only by the administrative API caller;
/// this database helper stores its digest.
pub fn create_runner_process_runtime_grant(
    pool: &DbPool,
    input: &NewRunnerProcessRuntimeGrant,
) -> RunnerVolumePurgeResult<RunnerProcessRuntimeGrant> {
    let (token_sha256, runtime_sha256) = validate_new_runner_process_runtime_grant(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let select = format!(
                "SELECT {PROCESS_RUNTIME_GRANT_COLUMNS} \
                   FROM jobs_runner_process_runtime_grants WHERE grant_id = ?1"
            );
            if let Some(stored) = tx
                .query_row(
                    &select,
                    params![input.grant_id],
                    process_runtime_grant_from_sqlite_row,
                )
                .optional()?
            {
                if !exact_process_runtime_grant(&stored, input, &token_sha256, &runtime_sha256) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            tx.execute(
                "INSERT INTO jobs_runner_process_runtime_grants ( \
                    grant_id, token_sha256, expected_worker_id, runtime_sha256, \
                    runner_image_sha256, runner_build_id, platform, architecture, \
                    automation_bundle_sha256, playwright_version, chromium_revision, \
                    chromium_executable_sha256, authorization_ref, created_by, \
                    expires_at_ms, created_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                           ?13, ?14, ?15, ?16)",
                params![
                    input.grant_id,
                    token_sha256,
                    input.expected_worker_id,
                    runtime_sha256,
                    input.runtime.runner_image_sha256,
                    input.runtime.runner_build_id,
                    input.runtime.platform,
                    input.runtime.architecture,
                    input.runtime.automation_bundle_sha256,
                    input.runtime.playwright_version,
                    input.runtime.chromium_revision,
                    input.runtime.chromium_executable_sha256,
                    input.authorization_ref,
                    input.created_by,
                    input.expires_at_ms,
                    input.created_at_ms,
                ],
            )?;
            let mut created = tx.query_row(
                &select,
                params![input.grant_id],
                process_runtime_grant_from_sqlite_row,
            )?;
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let select = format!(
                "SELECT {PROCESS_RUNTIME_GRANT_COLUMNS} \
                   FROM jobs_runner_process_runtime_grants WHERE grant_id = $1 FOR UPDATE"
            );
            if let Some(row) = tx.query_opt(&select, &[&input.grant_id])? {
                let stored = process_runtime_grant_from_pg_row(row);
                if !exact_process_runtime_grant(&stored, input, &token_sha256, &runtime_sha256) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            tx.execute(
                "INSERT INTO jobs_runner_process_runtime_grants ( \
                    grant_id, token_sha256, expected_worker_id, runtime_sha256, \
                    runner_image_sha256, runner_build_id, platform, architecture, \
                    automation_bundle_sha256, playwright_version, chromium_revision, \
                    chromium_executable_sha256, authorization_ref, created_by, \
                    expires_at_ms, created_at_ms \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                           $13, $14, $15, $16)",
                &[
                    &input.grant_id,
                    &token_sha256,
                    &input.expected_worker_id,
                    &runtime_sha256,
                    &input.runtime.runner_image_sha256,
                    &input.runtime.runner_build_id,
                    &input.runtime.platform,
                    &input.runtime.architecture,
                    &input.runtime.automation_bundle_sha256,
                    &input.runtime.playwright_version,
                    &input.runtime.chromium_revision,
                    &input.runtime.chromium_executable_sha256,
                    &input.authorization_ref,
                    &input.created_by,
                    &input.expires_at_ms,
                    &input.created_at_ms,
                ],
            )?;
            let mut created =
                process_runtime_grant_from_pg_row(tx.query_one(&select, &[&input.grant_id])?);
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
    })
}

fn validate_runner_process_runtime_grant_revocation(
    input: &RevokeRunnerProcessRuntimeGrantRequest,
) -> RunnerVolumePurgeResult<()> {
    require_runner_identifier(&input.grant_id)?;
    require_nonempty_text(&input.reason, 240)?;
    require_nonempty_text(&input.authorization_ref, 1_024)?;
    require_nonempty_text(&input.revoked_by, 240)?;
    if input.revoked_at_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn exact_runner_process_runtime_revocation(
    stored: &RunnerProcessRuntimeGrantRevocation,
    input: &RevokeRunnerProcessRuntimeGrantRequest,
) -> bool {
    stored.grant_id == input.grant_id
        && stored.reason == input.reason
        && stored.authorization_ref == input.authorization_ref
        && stored.revoked_by == input.revoked_by
}

/// Append-only cancellation for an unused deployment grant. Consumption and
/// cancellation serialize on the grant row, so exactly one can win. A bound
/// process is never retroactively erased because recovery must retain its exact
/// historical runtime evidence.
pub fn revoke_runner_process_runtime_grant(
    pool: &DbPool,
    input: &RevokeRunnerProcessRuntimeGrantRequest,
) -> RunnerVolumePurgeResult<RunnerProcessRuntimeGrantRevocation> {
    validate_runner_process_runtime_grant_revocation(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let grant_exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_runner_process_runtime_grants \
                  WHERE grant_id = ?1)",
                params![input.grant_id],
                |row| row.get(0),
            )?;
            if !grant_exists {
                return Err(RunnerVolumePurgeError::NotFound);
            }
            let stored = tx
                .query_row(
                    "SELECT grant_id, reason, authorization_ref, revoked_by, revoked_at_ms \
                       FROM jobs_runner_process_runtime_grant_revocations \
                      WHERE grant_id = ?1",
                    params![input.grant_id],
                    |row| {
                        Ok(RunnerProcessRuntimeGrantRevocation {
                            grant_id: row.get(0)?,
                            reason: row.get(1)?,
                            authorization_ref: row.get(2)?,
                            revoked_by: row.get(3)?,
                            revoked_at_ms: row.get(4)?,
                            disposition: RunnerVolumeWriteDisposition::Replay,
                        })
                    },
                )
                .optional()?;
            if let Some(stored) = stored {
                if !exact_runner_process_runtime_revocation(&stored, input) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            let consumed: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_runner_process_runtime_bindings \
                  WHERE grant_id = ?1)",
                params![input.grant_id],
                |row| row.get(0),
            )?;
            if consumed {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            tx.execute(
                "INSERT INTO jobs_runner_process_runtime_grant_revocations ( \
                    grant_id, reason, authorization_ref, revoked_by, revoked_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    input.grant_id,
                    input.reason,
                    input.authorization_ref,
                    input.revoked_by,
                    input.revoked_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(RunnerProcessRuntimeGrantRevocation {
                grant_id: input.grant_id.clone(),
                reason: input.reason.clone(),
                authorization_ref: input.authorization_ref.clone(),
                revoked_by: input.revoked_by.clone(),
                revoked_at_ms: input.revoked_at_ms,
                disposition: RunnerVolumeWriteDisposition::Applied,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_runner_process_runtime_grants \
                      WHERE grant_id = $1 FOR UPDATE",
                    &[&input.grant_id],
                )?
                .is_none()
            {
                return Err(RunnerVolumePurgeError::NotFound);
            }
            if let Some(row) = tx.query_opt(
                "SELECT grant_id, reason, authorization_ref, revoked_by, revoked_at_ms \
                   FROM jobs_runner_process_runtime_grant_revocations \
                  WHERE grant_id = $1",
                &[&input.grant_id],
            )? {
                let stored = RunnerProcessRuntimeGrantRevocation {
                    grant_id: row.get(0),
                    reason: row.get(1),
                    authorization_ref: row.get(2),
                    revoked_by: row.get(3),
                    revoked_at_ms: row.get(4),
                    disposition: RunnerVolumeWriteDisposition::Replay,
                };
                if !exact_runner_process_runtime_revocation(&stored, input) {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_runner_process_runtime_bindings \
                      WHERE grant_id = $1 FOR UPDATE",
                    &[&input.grant_id],
                )?
                .is_some()
            {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            tx.execute(
                "INSERT INTO jobs_runner_process_runtime_grant_revocations ( \
                    grant_id, reason, authorization_ref, revoked_by, revoked_at_ms \
                 ) VALUES ($1, $2, $3, $4, $5)",
                &[
                    &input.grant_id,
                    &input.reason,
                    &input.authorization_ref,
                    &input.revoked_by,
                    &input.revoked_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(RunnerProcessRuntimeGrantRevocation {
                grant_id: input.grant_id.clone(),
                reason: input.reason.clone(),
                authorization_ref: input.authorization_ref.clone(),
                revoked_by: input.revoked_by.clone(),
                revoked_at_ms: input.revoked_at_ms,
                disposition: RunnerVolumeWriteDisposition::Applied,
            })
        }
    })
}

fn trusted_process_runtime_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TrustedRunnerProcessRuntimeAttestation> {
    Ok(TrustedRunnerProcessRuntimeAttestation {
        runtime_grant_id: row.get(0)?,
        worker_id: row.get(1)?,
        volume_id: row.get(2)?,
        enrollment_epoch: row.get(3)?,
        process_instance_id: row.get(4)?,
        runtime_sha256: row.get(5)?,
        runtime: RunnerProcessRuntimeAttestation {
            runner_image_sha256: row.get(6)?,
            runner_build_id: row.get(7)?,
            platform: row.get(8)?,
            architecture: row.get(9)?,
            automation_bundle_sha256: row.get(10)?,
            playwright_version: row.get(11)?,
            chromium_revision: row.get(12)?,
            chromium_executable_sha256: row.get(13)?,
        },
        bound_at_ms: row.get(14)?,
    })
}

fn trusted_process_runtime_from_pg_row(
    row: postgres::Row,
) -> TrustedRunnerProcessRuntimeAttestation {
    TrustedRunnerProcessRuntimeAttestation {
        runtime_grant_id: row.get(0),
        worker_id: row.get(1),
        volume_id: row.get(2),
        enrollment_epoch: row.get(3),
        process_instance_id: row.get(4),
        runtime_sha256: row.get(5),
        runtime: RunnerProcessRuntimeAttestation {
            runner_image_sha256: row.get(6),
            runner_build_id: row.get(7),
            platform: row.get(8),
            architecture: row.get(9),
            automation_bundle_sha256: row.get(10),
            playwright_version: row.get(11),
            chromium_revision: row.get(12),
            chromium_executable_sha256: row.get(13),
        },
        bound_at_ms: row.get(14),
    }
}

const TRUSTED_PROCESS_RUNTIME_COLUMNS: &str =
    "b.grant_id, b.worker_id, b.volume_id, b.enrollment_epoch, \
     b.process_instance_id, b.runtime_sha256, g.runner_image_sha256, \
     g.runner_build_id, g.platform, g.architecture, g.automation_bundle_sha256, \
     g.playwright_version, g.chromium_revision, g.chromium_executable_sha256, \
     b.bound_at_ms";

fn validate_trusted_process_runtime(
    trusted: &TrustedRunnerProcessRuntimeAttestation,
) -> RunnerVolumePurgeResult<()> {
    require_runner_identifier(&trusted.runtime_grant_id)?;
    require_runner_identifier(&trusted.worker_id)?;
    require_base64url(&trusted.volume_id, 32)?;
    require_base64url(&trusted.process_instance_id, 32)?;
    if trusted.enrollment_epoch <= 0 || trusted.bound_at_ms < 0 {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let recomputed = runner_process_runtime_sha256(&trusted.runtime)?;
    if trusted.runtime_sha256 != recomputed {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

fn validate_runner_process_runtime_claim(
    claim: &RunnerProcessRuntimeGrantClaim,
) -> RunnerVolumePurgeResult<(String, String)> {
    require_runner_identifier(&claim.grant_id)?;
    let token = decode_base64url_exact(&claim.grant_token, 32)?;
    let runtime_sha256 = runner_process_runtime_sha256(&claim.runtime)?;
    Ok((hex::encode(Sha256::digest(token)), runtime_sha256))
}

fn runtime_grant_authorizes_claim(
    grant: &RunnerProcessRuntimeGrant,
    claim: &RunnerProcessRuntimeGrantClaim,
    expected_worker_id: &str,
    token_sha256: &str,
    runtime_sha256: &str,
) -> bool {
    grant.grant_id == claim.grant_id
        && grant.expected_worker_id == expected_worker_id
        && grant.runtime_sha256 == runtime_sha256
        && grant.runtime == claim.runtime
        && grant
            .token_sha256
            .as_bytes()
            .ct_eq(token_sha256.as_bytes())
            .unwrap_u8()
            == 1
}

fn exact_trusted_process_binding(
    trusted: &TrustedRunnerProcessRuntimeAttestation,
    grant_id: &str,
    worker_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    runtime_sha256: &str,
) -> bool {
    trusted.runtime_grant_id == grant_id
        && trusted.worker_id == worker_id
        && trusted.volume_id == volume_id
        && trusted.enrollment_epoch == enrollment_epoch
        && trusted.process_instance_id == process_instance_id
        && trusted.runtime_sha256 == runtime_sha256
}

fn bind_runner_process_runtime_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    claim: &RunnerProcessRuntimeGrantClaim,
    worker_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<TrustedRunnerProcessRuntimeAttestation> {
    let (token_sha256, runtime_sha256) = validate_runner_process_runtime_claim(claim)?;
    require_runner_identifier(worker_id)?;
    require_base64url(volume_id, 32)?;
    require_base64url(process_instance_id, 32)?;
    if enrollment_epoch <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    let grant_select = format!(
        "SELECT {PROCESS_RUNTIME_GRANT_COLUMNS} \
           FROM jobs_runner_process_runtime_grants WHERE grant_id = ?1"
    );
    let grant = tx
        .query_row(
            &grant_select,
            params![claim.grant_id],
            process_runtime_grant_from_sqlite_row,
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::Unauthorized)?;
    if !runtime_grant_authorizes_claim(&grant, claim, worker_id, &token_sha256, &runtime_sha256) {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let binding_select = format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
           FROM jobs_runner_process_runtime_bindings b \
           JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
          WHERE b.grant_id = ?1"
    );
    if let Some(trusted) = tx
        .query_row(
            &binding_select,
            params![claim.grant_id],
            trusted_process_runtime_from_sqlite_row,
        )
        .optional()?
    {
        validate_trusted_process_runtime(&trusted)?;
        if !exact_trusted_process_binding(
            &trusted,
            &claim.grant_id,
            worker_id,
            volume_id,
            enrollment_epoch,
            process_instance_id,
            &runtime_sha256,
        ) {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        return Ok(trusted);
    }
    let revoked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 \
           FROM jobs_runner_process_runtime_grant_revocations WHERE grant_id = ?1)",
        params![claim.grant_id],
        |row| row.get(0),
    )?;
    if revoked {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    if now_ms < grant.created_at_ms || now_ms > grant.expires_at_ms {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    if tx.execute(
        "INSERT OR IGNORE INTO jobs_runner_process_runtime_bindings ( \
            grant_id, worker_id, volume_id, enrollment_epoch, process_instance_id, \
            runtime_sha256, bound_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            claim.grant_id,
            worker_id,
            volume_id,
            enrollment_epoch,
            process_instance_id,
            runtime_sha256,
            now_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let trusted = tx.query_row(
        &binding_select,
        params![claim.grant_id],
        trusted_process_runtime_from_sqlite_row,
    )?;
    validate_trusted_process_runtime(&trusted)?;
    Ok(trusted)
}

fn bind_runner_process_runtime_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    claim: &RunnerProcessRuntimeGrantClaim,
    worker_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<TrustedRunnerProcessRuntimeAttestation> {
    let (token_sha256, runtime_sha256) = validate_runner_process_runtime_claim(claim)?;
    require_runner_identifier(worker_id)?;
    require_base64url(volume_id, 32)?;
    require_base64url(process_instance_id, 32)?;
    if enrollment_epoch <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    let grant_select = format!(
        "SELECT {PROCESS_RUNTIME_GRANT_COLUMNS} \
           FROM jobs_runner_process_runtime_grants WHERE grant_id = $1 FOR UPDATE"
    );
    let grant = tx
        .query_opt(&grant_select, &[&claim.grant_id])?
        .map(process_runtime_grant_from_pg_row)
        .ok_or(RunnerVolumePurgeError::Unauthorized)?;
    if !runtime_grant_authorizes_claim(&grant, claim, worker_id, &token_sha256, &runtime_sha256) {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let binding_select = format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
           FROM jobs_runner_process_runtime_bindings b \
           JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
          WHERE b.grant_id = $1 FOR UPDATE OF b"
    );
    if let Some(row) = tx.query_opt(&binding_select, &[&claim.grant_id])? {
        let trusted = trusted_process_runtime_from_pg_row(row);
        validate_trusted_process_runtime(&trusted)?;
        if !exact_trusted_process_binding(
            &trusted,
            &claim.grant_id,
            worker_id,
            volume_id,
            enrollment_epoch,
            process_instance_id,
            &runtime_sha256,
        ) {
            return Err(RunnerVolumePurgeError::Unauthorized);
        }
        return Ok(trusted);
    }
    if tx
        .query_opt(
            "SELECT 1 FROM jobs_runner_process_runtime_grant_revocations \
              WHERE grant_id = $1 FOR UPDATE",
            &[&claim.grant_id],
        )?
        .is_some()
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    if now_ms < grant.created_at_ms || now_ms > grant.expires_at_ms {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    if tx.execute(
        "INSERT INTO jobs_runner_process_runtime_bindings ( \
            grant_id, worker_id, volume_id, enrollment_epoch, process_instance_id, \
            runtime_sha256, bound_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT DO NOTHING",
        &[
            &claim.grant_id,
            &worker_id,
            &volume_id,
            &enrollment_epoch,
            &process_instance_id,
            &runtime_sha256,
            &now_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let trusted =
        trusted_process_runtime_from_pg_row(tx.query_one(&binding_select, &[&claim.grant_id])?);
    validate_trusted_process_runtime(&trusted)?;
    Ok(trusted)
}

pub(crate) fn require_runner_process_runtime_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    worker_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
) -> RunnerVolumePurgeResult<TrustedRunnerProcessRuntimeAttestation> {
    let select = format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
           FROM jobs_runner_process_runtime_bindings b \
           JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
          WHERE b.worker_id = ?1 AND b.volume_id = ?2 AND b.enrollment_epoch = ?3 \
            AND b.process_instance_id = ?4"
    );
    let trusted = tx
        .query_row(
            &select,
            params![worker_id, volume_id, enrollment_epoch, process_instance_id],
            trusted_process_runtime_from_sqlite_row,
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    validate_trusted_process_runtime(&trusted)?;
    Ok(trusted)
}

pub(crate) fn require_runner_process_runtime_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    worker_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
) -> RunnerVolumePurgeResult<TrustedRunnerProcessRuntimeAttestation> {
    let select = format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
           FROM jobs_runner_process_runtime_bindings b \
           JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
          WHERE b.worker_id = $1 AND b.volume_id = $2 AND b.enrollment_epoch = $3 \
            AND b.process_instance_id = $4 FOR UPDATE OF b"
    );
    let trusted = tx
        .query_opt(
            &select,
            &[
                &worker_id,
                &volume_id,
                &enrollment_epoch,
                &process_instance_id,
            ],
        )?
        .map(trusted_process_runtime_from_pg_row)
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    validate_trusted_process_runtime(&trusted)?;
    Ok(trusted)
}

pub(crate) fn bind_execution_lease_process_runtime_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    run_id: &str,
    fence: i64,
    trusted: &TrustedRunnerProcessRuntimeAttestation,
    bound_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    require_nonempty_text(run_id, 240)?;
    validate_trusted_process_runtime(trusted)?;
    if fence <= 0 || bound_at_ms < trusted.bound_at_ms {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    let existing = tx
        .query_row(
            "SELECT runtime_grant_id, worker_id, volume_id, enrollment_epoch, \
                    process_instance_id, runtime_sha256, bound_at_ms \
               FROM jobs_execution_lease_process_runtime_bindings \
              WHERE run_id = ?1 AND fence = ?2",
            params![run_id, fence],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing
            != (
                trusted.runtime_grant_id.clone(),
                trusted.worker_id.clone(),
                trusted.volume_id.clone(),
                trusted.enrollment_epoch,
                trusted.process_instance_id.clone(),
                trusted.runtime_sha256.clone(),
                bound_at_ms,
            )
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        return Ok(());
    }
    tx.execute(
        "INSERT INTO jobs_execution_lease_process_runtime_bindings ( \
            run_id, fence, runtime_grant_id, worker_id, volume_id, enrollment_epoch, \
            process_instance_id, runtime_sha256, bound_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            run_id,
            fence,
            trusted.runtime_grant_id,
            trusted.worker_id,
            trusted.volume_id,
            trusted.enrollment_epoch,
            trusted.process_instance_id,
            trusted.runtime_sha256,
            bound_at_ms,
        ],
    )?;
    Ok(())
}

pub(crate) fn bind_execution_lease_process_runtime_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    fence: i64,
    trusted: &TrustedRunnerProcessRuntimeAttestation,
    bound_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    require_nonempty_text(run_id, 240)?;
    validate_trusted_process_runtime(trusted)?;
    if fence <= 0 || bound_at_ms < trusted.bound_at_ms {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    if let Some(row) = tx.query_opt(
        "SELECT runtime_grant_id, worker_id, volume_id, enrollment_epoch, \
                process_instance_id, runtime_sha256, bound_at_ms \
           FROM jobs_execution_lease_process_runtime_bindings \
          WHERE run_id = $1 AND fence = $2 FOR UPDATE",
        &[&run_id, &fence],
    )? {
        let existing: (String, String, String, i64, String, String, i64) = (
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
            row.get(6),
        );
        if existing
            != (
                trusted.runtime_grant_id.clone(),
                trusted.worker_id.clone(),
                trusted.volume_id.clone(),
                trusted.enrollment_epoch,
                trusted.process_instance_id.clone(),
                trusted.runtime_sha256.clone(),
                bound_at_ms,
            )
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        return Ok(());
    }
    tx.execute(
        "INSERT INTO jobs_execution_lease_process_runtime_bindings ( \
            run_id, fence, runtime_grant_id, worker_id, volume_id, enrollment_epoch, \
            process_instance_id, runtime_sha256, bound_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &[
            &run_id,
            &fence,
            &trusted.runtime_grant_id,
            &trusted.worker_id,
            &trusted.volume_id,
            &trusted.enrollment_epoch,
            &trusted.process_instance_id,
            &trusted.runtime_sha256,
            &bound_at_ms,
        ],
    )?;
    Ok(())
}

/// Load current cloud runtime authority by execution fence. The returned
/// attestation is reconstructed from immutable server tables and is therefore
/// suitable for later ATS Phase A comparison. Lease/request JSON is never an
/// input to the runtime fields.
pub fn load_trusted_runner_process_runtime_attestation_for_execution(
    pool: &DbPool,
    run_id: &str,
    fence: i64,
    now_ms: i64,
) -> RunnerVolumePurgeResult<TrustedRunnerProcessRuntimeAttestation> {
    require_nonempty_text(run_id, 240)?;
    if fence <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let select = format!(
                "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
                   FROM jobs_execution_lease_process_runtime_bindings e \
                   JOIN jobs_runner_process_runtime_bindings b \
                     ON b.grant_id = e.runtime_grant_id \
                    AND b.worker_id = e.worker_id AND b.volume_id = e.volume_id \
                    AND b.enrollment_epoch = e.enrollment_epoch \
                    AND b.process_instance_id = e.process_instance_id \
                    AND b.runtime_sha256 = e.runtime_sha256 \
                   JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
                   JOIN jobs_execution_leases l ON l.run_id = e.run_id AND l.fence = e.fence \
                  WHERE e.run_id = ?1 AND e.fence = ?2 \
                    AND l.phase IN ('prepared', 'click_started') \
                    AND l.lease_expires_at_ms > ?3"
            );
            let trusted = conn
                .query_row(
                    &select,
                    params![run_id, fence, now_ms],
                    trusted_process_runtime_from_sqlite_row,
                )
                .optional()?
                .ok_or(RunnerVolumePurgeError::NotReady)?;
            validate_trusted_process_runtime(&trusted)?;
            Ok(trusted)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let select = format!(
                "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} \
                   FROM jobs_execution_lease_process_runtime_bindings e \
                   JOIN jobs_runner_process_runtime_bindings b \
                     ON b.grant_id = e.runtime_grant_id \
                    AND b.worker_id = e.worker_id AND b.volume_id = e.volume_id \
                    AND b.enrollment_epoch = e.enrollment_epoch \
                    AND b.process_instance_id = e.process_instance_id \
                    AND b.runtime_sha256 = e.runtime_sha256 \
                   JOIN jobs_runner_process_runtime_grants g ON g.grant_id = b.grant_id \
                   JOIN jobs_execution_leases l ON l.run_id = e.run_id AND l.fence = e.fence \
                  WHERE e.run_id = $1 AND e.fence = $2 \
                    AND l.phase IN ('prepared', 'click_started') \
                    AND l.lease_expires_at_ms > $3 FOR UPDATE OF e, b, l"
            );
            let trusted = tx
                .query_opt(&select, &[&run_id, &fence, &now_ms])?
                .map(trusted_process_runtime_from_pg_row)
                .ok_or(RunnerVolumePurgeError::NotReady)?;
            validate_trusted_process_runtime(&trusted)?;
            tx.commit()?;
            Ok(trusted)
        }
    })
}

const RUNNER_VOLUME_COLUMNS: &str =
    "v.volume_id, v.worker_id, v.provider, v.provider_resource_id, \
     v.resource_fingerprint, v.current_epoch, v.enrollment_generation, \
     v.required_tombstone_generation, v.reconciled_tombstone_generation, \
     v.status, v.active_instance_id, v.instance_lease_expires_at_ms, \
     v.legacy_artifact_count, v.admission_grant_id, v.enrolled_at_ms, \
     v.last_seen_at_ms, v.updated_at_ms, k.public_key_base64url, k.key_fingerprint";

fn runner_volume_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunnerVolumeRecord> {
    Ok(RunnerVolumeRecord {
        volume_id: row.get(0)?,
        worker_id: row.get(1)?,
        provider: row.get(2)?,
        provider_resource_id: row.get(3)?,
        resource_fingerprint: row.get(4)?,
        current_epoch: row.get(5)?,
        enrollment_generation: row.get(6)?,
        required_tombstone_generation: row.get(7)?,
        reconciled_tombstone_generation: row.get(8)?,
        status: row.get(9)?,
        active_instance_id: row.get(10)?,
        instance_lease_expires_at_ms: row.get(11)?,
        legacy_artifact_count: row.get(12)?,
        admission_grant_id: row.get(13)?,
        enrolled_at_ms: row.get(14)?,
        last_seen_at_ms: row.get(15)?,
        updated_at_ms: row.get(16)?,
        public_key_base64url: row.get(17)?,
        key_fingerprint: row.get(18)?,
        disposition: RunnerVolumeWriteDisposition::Replay,
    })
}

fn runner_volume_from_pg_row(row: postgres::Row) -> RunnerVolumeRecord {
    RunnerVolumeRecord {
        volume_id: row.get(0),
        worker_id: row.get(1),
        provider: row.get(2),
        provider_resource_id: row.get(3),
        resource_fingerprint: row.get(4),
        current_epoch: row.get(5),
        enrollment_generation: row.get(6),
        required_tombstone_generation: row.get(7),
        reconciled_tombstone_generation: row.get(8),
        status: row.get(9),
        active_instance_id: row.get(10),
        instance_lease_expires_at_ms: row.get(11),
        legacy_artifact_count: row.get(12),
        admission_grant_id: row.get(13),
        enrolled_at_ms: row.get(14),
        last_seen_at_ms: row.get(15),
        updated_at_ms: row.get(16),
        public_key_base64url: row.get(17),
        key_fingerprint: row.get(18),
        disposition: RunnerVolumeWriteDisposition::Replay,
    }
}

pub fn lookup_runner_volume(
    pool: &DbPool,
    volume_id: &str,
) -> RunnerVolumePurgeResult<Option<RunnerVolumeRecord>> {
    require_base64url(volume_id, 32)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let select = format!(
                "SELECT {RUNNER_VOLUME_COLUMNS} \
                   FROM jobs_runner_volumes v JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                  WHERE v.volume_id = ?1"
            );
            Ok(conn
                .query_row(&select, params![volume_id], runner_volume_from_sqlite_row)
                .optional()?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let select = format!(
                "SELECT {RUNNER_VOLUME_COLUMNS} \
                   FROM jobs_runner_volumes v JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                  WHERE v.volume_id = $1"
            );
            Ok(conn
                .query_opt(&select, &[&volume_id])?
                .map(runner_volume_from_pg_row))
        }
    })
}

const RUNNER_VOLUME_AUTHORITY_USE_RETENTION_MS: i64 = 10 * 60 * 1_000;

#[derive(Debug, Clone)]
pub struct VerifiedRunnerVolumeAuthority {
    volume: RunnerVolumeRecord,
    request_id: String,
    operation: String,
    payload_sha256: String,
    process_instance_id: String,
    issued_at_ms: i64,
    maximum_clock_skew_ms: i64,
}

impl VerifiedRunnerVolumeAuthority {
    pub fn volume(&self) -> &RunnerVolumeRecord {
        &self.volume
    }
}

pub fn verify_runner_volume_authority_proof(
    pool: &DbPool,
    proof: &RunnerVolumeAuthorityProof,
    expected_operation: &str,
    expected_payload_sha256: &str,
    server_now_ms: i64,
    maximum_clock_skew_ms: i64,
) -> RunnerVolumePurgeResult<VerifiedRunnerVolumeAuthority> {
    proof.validate_unsigned()?;
    require_sha256(expected_payload_sha256)?;
    if proof.operation != expected_operation
        || proof.payload_sha256 != expected_payload_sha256
        || server_now_ms < 0
        || maximum_clock_skew_ms < 0
        || server_now_ms.abs_diff(proof.issued_at_ms)
            > u64::try_from(maximum_clock_skew_ms)
                .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let volume =
        lookup_runner_volume(pool, &proof.volume_id)?.ok_or(RunnerVolumePurgeError::NotFound)?;
    if volume.current_epoch != proof.enrollment_epoch || volume.status == "destroyed" {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    verify_runner_volume_authority_signature(proof, &volume.public_key_base64url)?;
    Ok(VerifiedRunnerVolumeAuthority {
        volume,
        request_id: proof.request_id.clone(),
        operation: proof.operation.clone(),
        payload_sha256: proof.payload_sha256.clone(),
        process_instance_id: proof.process_instance_id.clone(),
        issued_at_ms: proof.issued_at_ms,
        maximum_clock_skew_ms,
    })
}

fn validate_runner_volume_authority_use(
    authority: &VerifiedRunnerVolumeAuthority,
    expected_operation: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    consumed_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    if consumed_at_ms < 0
        || authority.operation != expected_operation
        || authority.volume.volume_id != volume_id
        || authority.volume.current_epoch != enrollment_epoch
        || authority.process_instance_id != process_instance_id
        || consumed_at_ms.abs_diff(authority.issued_at_ms)
            > u64::try_from(authority.maximum_clock_skew_ms)
                .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

fn consume_runner_volume_authority_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    authority: &VerifiedRunnerVolumeAuthority,
    expected_operation: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    consumed_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    validate_runner_volume_authority_use(
        authority,
        expected_operation,
        volume_id,
        enrollment_epoch,
        process_instance_id,
        consumed_at_ms,
    )?;
    let expired_before_ms = consumed_at_ms.saturating_sub(RUNNER_VOLUME_AUTHORITY_USE_RETENTION_MS);
    tx.execute(
        "DELETE FROM jobs_runner_volume_authority_uses WHERE consumed_at_ms < ?1",
        params![expired_before_ms],
    )?;
    if tx.execute(
        "INSERT OR IGNORE INTO jobs_runner_volume_authority_uses ( \
            volume_id, enrollment_epoch, request_id, operation, payload_sha256, \
            process_instance_id, issued_at_ms, consumed_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            volume_id,
            enrollment_epoch,
            authority.request_id,
            authority.operation,
            authority.payload_sha256,
            process_instance_id,
            authority.issued_at_ms,
            consumed_at_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

pub(crate) fn prelock_runner_volume_authority_use_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<()> {
    let namespace = format!(
        "jobs-runner-volume-authority-use:{}:{}:{}",
        authority.volume.volume_id, authority.volume.current_epoch, authority.request_id
    );
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&namespace],
    )?;
    Ok(())
}

fn consume_runner_volume_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    authority: &VerifiedRunnerVolumeAuthority,
    expected_operation: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    consumed_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    validate_runner_volume_authority_use(
        authority,
        expected_operation,
        volume_id,
        enrollment_epoch,
        process_instance_id,
        consumed_at_ms,
    )?;
    prelock_runner_volume_authority_use_postgres_tx(tx, authority)?;
    if tx.execute(
        "INSERT INTO jobs_runner_volume_authority_uses ( \
            volume_id, enrollment_epoch, request_id, operation, payload_sha256, \
            process_instance_id, issued_at_ms, consumed_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT(volume_id, enrollment_epoch, request_id) DO NOTHING",
        &[
            &volume_id,
            &enrollment_epoch,
            &authority.request_id,
            &authority.operation,
            &authority.payload_sha256,
            &process_instance_id,
            &authority.issued_at_ms,
            &consumed_at_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerLegacyInventoryAuthorityRecord {
    pub authority_id: String,
    pub reconciliation_id: String,
    pub authority_generation: i64,
    pub authority_state: String,
    pub predecessor_generation: i64,
    pub predecessor_authority_id: Option<String>,
    pub predecessor_authority_sha256: Option<String>,
    pub root_count: i64,
    pub root_set_sha256: String,
    pub scope_ref: String,
    pub evidence_ref: String,
    pub evidence_sha256: String,
    pub authorized_by: String,
    pub recorded_at_ms: i64,
    pub authority_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordRunnerLegacyInventoryAuthorityRequest {
    pub reconciliation_id: String,
    pub authority_state: String,
    pub expected_predecessor_generation: i64,
    pub expected_predecessor_authority_id: Option<String>,
    pub expected_predecessor_authority_sha256: Option<String>,
    pub root_count: i64,
    pub root_set_sha256: String,
    pub scope_ref: String,
    pub evidence_ref: String,
    pub evidence_sha256: String,
    pub authorized_by: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerLegacyInventoryAuthorityOutcome {
    pub disposition: RunnerVolumeWriteDisposition,
    pub authority: RunnerLegacyInventoryAuthorityRecord,
}

fn validate_legacy_inventory_authority_request(
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> RunnerVolumePurgeResult<()> {
    require_runner_identifier(&input.reconciliation_id)?;
    if !matches!(input.authority_state.as_str(), "reconciling" | "ready")
        || input.expected_predecessor_generation < 0
        || input.root_count < 0
        || input.recorded_at_ms < 0
        || (input.root_count == 0 && input.root_set_sha256 != EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256)
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    require_sha256(&input.root_set_sha256)?;
    require_nonempty_text(&input.scope_ref, 2_048)?;
    require_nonempty_text(&input.evidence_ref, 2_048)?;
    require_sha256(&input.evidence_sha256)?;
    require_nonempty_text(&input.authorized_by, 240)?;
    match (
        input.expected_predecessor_generation,
        input.expected_predecessor_authority_id.as_deref(),
        input.expected_predecessor_authority_sha256.as_deref(),
    ) {
        (0, None, None) => {}
        (generation, Some(authority_id), Some(authority_sha256)) if generation > 0 => {
            require_runner_identifier(authority_id)?;
            require_sha256(authority_sha256)?;
        }
        _ => return Err(RunnerVolumePurgeError::InvalidRequest),
    }
    if input.authority_state == "ready" && input.expected_predecessor_generation == 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

fn legacy_inventory_authority_id(reconciliation_id: &str, authority_state: &str) -> String {
    let material = format!(
        "{RUNNER_LEGACY_INVENTORY_ID_DOMAIN}\nreconciliation_id={reconciliation_id}\nauthority_state={authority_state}\n"
    );
    format!("legacy-inventory-{}", hex::encode(Sha256::digest(material)))
}

fn legacy_inventory_authority_sha256(authority: &RunnerLegacyInventoryAuthorityRecord) -> String {
    let predecessor_authority_id = authority.predecessor_authority_id.as_deref().unwrap_or("");
    let predecessor_authority_sha256 = authority
        .predecessor_authority_sha256
        .as_deref()
        .unwrap_or("");
    let canonical = format!(
        concat!(
            "{}\n",
            "authority_id={}\n",
            "reconciliation_id={}\n",
            "authority_generation={}\n",
            "authority_state={}\n",
            "predecessor_generation={}\n",
            "predecessor_authority_id={}\n",
            "predecessor_authority_sha256={}\n",
            "root_count={}\n",
            "root_set_sha256={}\n",
            "scope_ref={}\n",
            "evidence_ref={}\n",
            "evidence_sha256={}\n",
            "authorized_by={}\n",
            "recorded_at_ms={}\n"
        ),
        RUNNER_LEGACY_INVENTORY_AUTHORITY_DOMAIN,
        authority.authority_id,
        authority.reconciliation_id,
        authority.authority_generation,
        authority.authority_state,
        authority.predecessor_generation,
        predecessor_authority_id,
        predecessor_authority_sha256,
        authority.root_count,
        authority.root_set_sha256,
        authority.scope_ref,
        authority.evidence_ref,
        authority.evidence_sha256,
        authority.authorized_by,
        authority.recorded_at_ms,
    );
    hex::encode(Sha256::digest(canonical))
}

fn legacy_inventory_authority_matches_request(
    authority: &RunnerLegacyInventoryAuthorityRecord,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> bool {
    authority.reconciliation_id == input.reconciliation_id
        && authority.authority_state == input.authority_state
        && authority.predecessor_generation == input.expected_predecessor_generation
        && authority.predecessor_authority_id == input.expected_predecessor_authority_id
        && authority.predecessor_authority_sha256 == input.expected_predecessor_authority_sha256
        && authority.root_count == input.root_count
        && authority.root_set_sha256 == input.root_set_sha256
        && authority.scope_ref == input.scope_ref
        && authority.evidence_ref == input.evidence_ref
        && authority.evidence_sha256 == input.evidence_sha256
        && authority.authorized_by == input.authorized_by
        && authority.authority_sha256 == legacy_inventory_authority_sha256(authority)
}

fn legacy_inventory_authority_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RunnerLegacyInventoryAuthorityRecord> {
    Ok(RunnerLegacyInventoryAuthorityRecord {
        authority_id: row.get(0)?,
        reconciliation_id: row.get(1)?,
        authority_generation: row.get(2)?,
        authority_state: row.get(3)?,
        predecessor_generation: row.get(4)?,
        predecessor_authority_id: row.get(5)?,
        predecessor_authority_sha256: row.get(6)?,
        root_count: row.get(7)?,
        root_set_sha256: row.get(8)?,
        scope_ref: row.get(9)?,
        evidence_ref: row.get(10)?,
        evidence_sha256: row.get(11)?,
        authorized_by: row.get(12)?,
        recorded_at_ms: row.get(13)?,
        authority_sha256: row.get(14)?,
    })
}

fn legacy_inventory_authority_from_pg_row(
    row: postgres::Row,
) -> RunnerLegacyInventoryAuthorityRecord {
    RunnerLegacyInventoryAuthorityRecord {
        authority_id: row.get(0),
        reconciliation_id: row.get(1),
        authority_generation: row.get(2),
        authority_state: row.get(3),
        predecessor_generation: row.get(4),
        predecessor_authority_id: row.get(5),
        predecessor_authority_sha256: row.get(6),
        root_count: row.get(7),
        root_set_sha256: row.get(8),
        scope_ref: row.get(9),
        evidence_ref: row.get(10),
        evidence_sha256: row.get(11),
        authorized_by: row.get(12),
        recorded_at_ms: row.get(13),
        authority_sha256: row.get(14),
    }
}

const LEGACY_INVENTORY_AUTHORITY_COLUMNS: &str =
    "authority_id, reconciliation_id, authority_generation, authority_state, \
     predecessor_generation, predecessor_authority_id, predecessor_authority_sha256, \
     root_count, root_set_sha256, scope_ref, evidence_ref, evidence_sha256, authorized_by, \
     recorded_at_ms, authority_sha256";

#[derive(Debug)]
struct CurrentLegacyInventoryAuthority {
    state: String,
    generation: i64,
    reconciliation_id: Option<String>,
    authority_id: Option<String>,
    authority_sha256: Option<String>,
}

fn legacy_inventory_predecessor_matches(
    current: &CurrentLegacyInventoryAuthority,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> bool {
    current.generation == input.expected_predecessor_generation
        && current.authority_id == input.expected_predecessor_authority_id
        && current.authority_sha256 == input.expected_predecessor_authority_sha256
}

fn validate_legacy_inventory_transition(
    current: &CurrentLegacyInventoryAuthority,
    predecessor: Option<&RunnerLegacyInventoryAuthorityRecord>,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> RunnerVolumePurgeResult<()> {
    if !legacy_inventory_predecessor_matches(current, input) {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    match input.authority_state.as_str() {
        "reconciling"
            if matches!(current.state.as_str(), "unknown" | "ready")
                || (current.state == "reconciling"
                    && predecessor
                        .is_some_and(|authority| authority.authority_state == "ready")) =>
        {
            Ok(())
        }
        "ready"
            if current.state == "reconciling"
                && current.reconciliation_id.as_deref()
                    == Some(input.reconciliation_id.as_str())
                && predecessor.is_some_and(|authority| {
                    authority.authority_state == "reconciling"
                        && authority.reconciliation_id == input.reconciliation_id
                        && authority.root_count == input.root_count
                        && authority.root_set_sha256 == input.root_set_sha256
                        && authority.scope_ref == input.scope_ref
                }) =>
        {
            Ok(())
        }
        "ready" => Err(RunnerVolumePurgeError::NotReady),
        _ => Err(RunnerVolumePurgeError::Conflict),
    }
}

pub fn record_runner_legacy_inventory_authority(
    pool: &DbPool,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> RunnerVolumePurgeResult<RunnerLegacyInventoryAuthorityOutcome> {
    validate_legacy_inventory_authority_request(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_runner_legacy_inventory_authority_sqlite(pool, input),
        DbPool::Postgres(_) => record_runner_legacy_inventory_authority_postgres(pool, input),
    })
}

fn record_runner_legacy_inventory_authority_sqlite(
    pool: &DbPool,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> RunnerVolumePurgeResult<RunnerLegacyInventoryAuthorityOutcome> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let select_existing = format!(
        "SELECT {LEGACY_INVENTORY_AUTHORITY_COLUMNS} \
           FROM jobs_runner_legacy_inventory_authorities \
          WHERE reconciliation_id = ?1 AND authority_state = ?2"
    );
    if let Some(authority) = tx
        .query_row(
            &select_existing,
            params![input.reconciliation_id, input.authority_state],
            legacy_inventory_authority_from_sqlite_row,
        )
        .optional()?
    {
        if !legacy_inventory_authority_matches_request(&authority, input) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(RunnerLegacyInventoryAuthorityOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            authority,
        });
    }
    let current = tx.query_row(
        "SELECT legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| {
            Ok(CurrentLegacyInventoryAuthority {
                state: row.get(0)?,
                generation: row.get(1)?,
                reconciliation_id: row.get(2)?,
                authority_id: row.get(3)?,
                authority_sha256: row.get(4)?,
            })
        },
    )?;
    let predecessor = current
        .authority_id
        .as_ref()
        .map(|authority_id| {
            let select = format!(
                "SELECT {LEGACY_INVENTORY_AUTHORITY_COLUMNS} \
                   FROM jobs_runner_legacy_inventory_authorities WHERE authority_id = ?1"
            );
            tx.query_row(
                &select,
                params![authority_id],
                legacy_inventory_authority_from_sqlite_row,
            )
        })
        .transpose()?;
    validate_legacy_inventory_transition(&current, predecessor.as_ref(), input)?;
    let authority_generation = current
        .generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let authority_id =
        legacy_inventory_authority_id(&input.reconciliation_id, &input.authority_state);
    let mut authority = RunnerLegacyInventoryAuthorityRecord {
        authority_id,
        reconciliation_id: input.reconciliation_id.clone(),
        authority_generation,
        authority_state: input.authority_state.clone(),
        predecessor_generation: input.expected_predecessor_generation,
        predecessor_authority_id: input.expected_predecessor_authority_id.clone(),
        predecessor_authority_sha256: input.expected_predecessor_authority_sha256.clone(),
        root_count: input.root_count,
        root_set_sha256: input.root_set_sha256.clone(),
        scope_ref: input.scope_ref.clone(),
        evidence_ref: input.evidence_ref.clone(),
        evidence_sha256: input.evidence_sha256.clone(),
        authorized_by: input.authorized_by.clone(),
        recorded_at_ms: input.recorded_at_ms,
        authority_sha256: String::new(),
    };
    authority.authority_sha256 = legacy_inventory_authority_sha256(&authority);
    tx.execute(
        "INSERT INTO jobs_runner_legacy_inventory_authorities ( \
            authority_id, reconciliation_id, authority_generation, authority_state, \
            predecessor_generation, predecessor_authority_id, predecessor_authority_sha256, \
            root_count, root_set_sha256, scope_ref, evidence_ref, evidence_sha256, \
            authorized_by, recorded_at_ms, authority_sha256 \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            authority.authority_id,
            authority.reconciliation_id,
            authority.authority_generation,
            authority.authority_state,
            authority.predecessor_generation,
            authority.predecessor_authority_id,
            authority.predecessor_authority_sha256,
            authority.root_count,
            authority.root_set_sha256,
            authority.scope_ref,
            authority.evidence_ref,
            authority.evidence_sha256,
            authority.authorized_by,
            authority.recorded_at_ms,
            authority.authority_sha256,
        ],
    )?;
    invalidate_sqlite_runner_fleet_cutover(&tx, input.recorded_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state SET \
            legacy_inventory_state = ?1, legacy_inventory_generation = ?2, \
            legacy_inventory_reconciliation_id = ?3, legacy_inventory_authority_id = ?4, \
            legacy_inventory_authority_sha256 = ?5, legacy_inventory_root_count = ?6, \
            legacy_inventory_root_set_sha256 = ?7, unresolved_legacy_volume_count = 0, \
            updated_at_ms = ?8 \
          WHERE singleton_id = 1 AND legacy_inventory_generation = ?9",
        params![
            authority.authority_state,
            authority.authority_generation,
            authority.reconciliation_id,
            authority.authority_id,
            authority.authority_sha256,
            authority.root_count,
            authority.root_set_sha256,
            input.recorded_at_ms,
            current.generation,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerLegacyInventoryAuthorityOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        authority,
    })
}

fn record_runner_legacy_inventory_authority_postgres(
    pool: &DbPool,
    input: &RecordRunnerLegacyInventoryAuthorityRequest,
) -> RunnerVolumePurgeResult<RunnerLegacyInventoryAuthorityOutcome> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    // Serialize every authority attempt on the fleet singleton before the
    // idempotency lookup. Otherwise two first-phase requests can both observe
    // a missing row and race to allocate the same successor generation.
    let row = tx.query_one(
        "SELECT legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let select_existing = format!(
        "SELECT {LEGACY_INVENTORY_AUTHORITY_COLUMNS} \
           FROM jobs_runner_legacy_inventory_authorities \
          WHERE reconciliation_id = $1 AND authority_state = $2 FOR UPDATE"
    );
    if let Some(row) = tx.query_opt(
        &select_existing,
        &[&input.reconciliation_id, &input.authority_state],
    )? {
        let authority = legacy_inventory_authority_from_pg_row(row);
        if !legacy_inventory_authority_matches_request(&authority, input) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(RunnerLegacyInventoryAuthorityOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            authority,
        });
    }
    let current = CurrentLegacyInventoryAuthority {
        state: row.get(0),
        generation: row.get(1),
        reconciliation_id: row.get(2),
        authority_id: row.get(3),
        authority_sha256: row.get(4),
    };
    let predecessor = if let Some(authority_id) = current.authority_id.as_ref() {
        let select = format!(
            "SELECT {LEGACY_INVENTORY_AUTHORITY_COLUMNS} \
               FROM jobs_runner_legacy_inventory_authorities \
              WHERE authority_id = $1 FOR UPDATE"
        );
        Some(legacy_inventory_authority_from_pg_row(
            tx.query_one(&select, &[authority_id])?,
        ))
    } else {
        None
    };
    validate_legacy_inventory_transition(&current, predecessor.as_ref(), input)?;
    let authority_generation = current
        .generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let authority_id =
        legacy_inventory_authority_id(&input.reconciliation_id, &input.authority_state);
    let mut authority = RunnerLegacyInventoryAuthorityRecord {
        authority_id,
        reconciliation_id: input.reconciliation_id.clone(),
        authority_generation,
        authority_state: input.authority_state.clone(),
        predecessor_generation: input.expected_predecessor_generation,
        predecessor_authority_id: input.expected_predecessor_authority_id.clone(),
        predecessor_authority_sha256: input.expected_predecessor_authority_sha256.clone(),
        root_count: input.root_count,
        root_set_sha256: input.root_set_sha256.clone(),
        scope_ref: input.scope_ref.clone(),
        evidence_ref: input.evidence_ref.clone(),
        evidence_sha256: input.evidence_sha256.clone(),
        authorized_by: input.authorized_by.clone(),
        recorded_at_ms: input.recorded_at_ms,
        authority_sha256: String::new(),
    };
    authority.authority_sha256 = legacy_inventory_authority_sha256(&authority);
    tx.execute(
        "INSERT INTO jobs_runner_legacy_inventory_authorities ( \
            authority_id, reconciliation_id, authority_generation, authority_state, \
            predecessor_generation, predecessor_authority_id, predecessor_authority_sha256, \
            root_count, root_set_sha256, scope_ref, evidence_ref, evidence_sha256, \
            authorized_by, recorded_at_ms, authority_sha256 \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        &[
            &authority.authority_id,
            &authority.reconciliation_id,
            &authority.authority_generation,
            &authority.authority_state,
            &authority.predecessor_generation,
            &authority.predecessor_authority_id,
            &authority.predecessor_authority_sha256,
            &authority.root_count,
            &authority.root_set_sha256,
            &authority.scope_ref,
            &authority.evidence_ref,
            &authority.evidence_sha256,
            &authority.authorized_by,
            &authority.recorded_at_ms,
            &authority.authority_sha256,
        ],
    )?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, input.recorded_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state SET \
            legacy_inventory_state = $1, legacy_inventory_generation = $2, \
            legacy_inventory_reconciliation_id = $3, legacy_inventory_authority_id = $4, \
            legacy_inventory_authority_sha256 = $5, legacy_inventory_root_count = $6, \
            legacy_inventory_root_set_sha256 = $7, unresolved_legacy_volume_count = 0, \
            updated_at_ms = $8 \
          WHERE singleton_id = 1 AND legacy_inventory_generation = $9",
        &[
            &authority.authority_state,
            &authority.authority_generation,
            &authority.reconciliation_id,
            &authority.authority_id,
            &authority.authority_sha256,
            &authority.root_count,
            &authority.root_set_sha256,
            &input.recorded_at_ms,
            &current.generation,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerLegacyInventoryAuthorityOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        authority,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeStorageAttestationOutcome {
    pub disposition: RunnerVolumeWriteDisposition,
    pub attestation_generation: i64,
    pub fleet_attestation_generation: i64,
    pub attestation_sha256: String,
    pub volume_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeStorageAttestationPredecessor {
    pub enrollment_generation: i64,
    pub predecessor_attestation_generation: i64,
    pub predecessor_attestation_sha256: String,
    pub storage_attestation_required: bool,
}

#[derive(Debug)]
struct RunnerStorageAttestationSetEntry {
    volume_id: String,
    enrollment_epoch: i64,
    volume_key_fingerprint: String,
    attestation_generation: i64,
    attestation_sha256: String,
}

fn runner_storage_attestation_set_sha256(
    entries: &[RunnerStorageAttestationSetEntry],
) -> RunnerVolumePurgeResult<String> {
    let mut digest = Sha256::new();
    digest.update(RUNNER_STORAGE_ATTESTATION_SET_DOMAIN.as_bytes());
    digest.update(b"\n");
    let mut previous_volume_id: Option<&str> = None;
    for entry in entries {
        require_base64url(&entry.volume_id, 32)?;
        require_sha256(&entry.volume_key_fingerprint)?;
        require_sha256(&entry.attestation_sha256)?;
        if entry.enrollment_epoch <= 0
            || entry.attestation_generation <= 0
            || previous_volume_id.is_some_and(|previous| previous >= entry.volume_id.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        digest.update(b"volume_id=");
        digest.update(entry.volume_id.as_bytes());
        digest.update(b"\nenrollment_epoch=");
        digest.update(entry.enrollment_epoch.to_string().as_bytes());
        digest.update(b"\nvolume_key_fingerprint=");
        digest.update(entry.volume_key_fingerprint.as_bytes());
        digest.update(b"\nattestation_generation=");
        digest.update(entry.attestation_generation.to_string().as_bytes());
        digest.update(b"\nattestation_sha256=");
        digest.update(entry.attestation_sha256.as_bytes());
        digest.update(b"\n");
        previous_volume_id = Some(&entry.volume_id);
    }
    Ok(hex::encode(digest.finalize()))
}

fn sqlite_runner_storage_attestation_set(
    tx: &RunnerSqliteTransaction<'_>,
) -> RunnerVolumePurgeResult<(i64, String)> {
    let mut statement = tx.prepare(
        "SELECT v.volume_id, v.current_epoch, k.key_fingerprint, \
                a.attestation_generation, a.attestation_sha256 \
           FROM jobs_runner_volumes v \
           JOIN jobs_runner_volume_keys k \
             ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
           JOIN jobs_runner_volume_storage_attestations a \
             ON a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
          WHERE v.status <> 'destroyed' \
            AND NOT EXISTS ( \
              SELECT 1 FROM jobs_runner_volume_storage_attestations newer \
               WHERE newer.volume_id = a.volume_id \
                 AND newer.enrollment_epoch = a.enrollment_epoch \
                 AND newer.attestation_generation > a.attestation_generation \
            ) \
          ORDER BY v.volume_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RunnerStorageAttestationSetEntry {
            volume_id: row.get(0)?,
            enrollment_epoch: row.get(1)?,
            volume_key_fingerprint: row.get(2)?,
            attestation_generation: row.get(3)?,
            attestation_sha256: row.get(4)?,
        })
    })?;
    let entries = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let count = i64::try_from(entries.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    Ok((count, runner_storage_attestation_set_sha256(&entries)?))
}

fn postgres_runner_storage_attestation_set(
    tx: &mut postgres::Transaction<'_>,
) -> RunnerVolumePurgeResult<(i64, String)> {
    let entries = tx
        .query(
            "SELECT v.volume_id, v.current_epoch, k.key_fingerprint, \
                    a.attestation_generation, a.attestation_sha256 \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
               JOIN jobs_runner_volume_storage_attestations a \
                 ON a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
              WHERE v.status <> 'destroyed' \
                AND NOT EXISTS ( \
                  SELECT 1 FROM jobs_runner_volume_storage_attestations newer \
                   WHERE newer.volume_id = a.volume_id \
                     AND newer.enrollment_epoch = a.enrollment_epoch \
                     AND newer.attestation_generation > a.attestation_generation \
                ) \
              ORDER BY v.volume_id",
            &[],
        )?
        .into_iter()
        .map(|row| RunnerStorageAttestationSetEntry {
            volume_id: row.get(0),
            enrollment_epoch: row.get(1),
            volume_key_fingerprint: row.get(2),
            attestation_generation: row.get(3),
            attestation_sha256: row.get(4),
        })
        .collect::<Vec<_>>();
    let count = i64::try_from(entries.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    Ok((count, runner_storage_attestation_set_sha256(&entries)?))
}

pub fn runner_volume_storage_attestation_predecessor(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestationPredecessor> {
    require_base64url(volume_id, 32)?;
    require_base64url(process_instance_id, 32)?;
    if enrollment_epoch <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let row = conn
                .query_row(
                    "SELECT v.current_epoch, v.enrollment_generation, v.status, \
                            v.active_instance_id, v.instance_lease_expires_at_ms, \
                            v.required_tombstone_generation, \
                            v.reconciled_tombstone_generation, k.key_fingerprint, \
                            a.attestation_generation, a.attestation_sha256, \
                            a.process_instance_id, a.enrollment_generation, \
                            a.required_tombstone_generation, \
                            a.reconciled_tombstone_generation, a.volume_key_fingerprint \
                       FROM jobs_runner_volumes v \
                       JOIN jobs_runner_volume_keys k \
                         ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                       LEFT JOIN jobs_runner_volume_storage_attestations a \
                         ON a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                        AND NOT EXISTS ( \
                          SELECT 1 FROM jobs_runner_volume_storage_attestations newer \
                           WHERE newer.volume_id = a.volume_id \
                             AND newer.enrollment_epoch = a.enrollment_epoch \
                             AND newer.attestation_generation > a.attestation_generation) \
                      WHERE v.volume_id = ?1",
                    params![volume_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<i64>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, Option<i64>>(8)?,
                            row.get::<_, Option<String>>(9)?,
                            row.get::<_, Option<String>>(10)?,
                            row.get::<_, Option<i64>>(11)?,
                            row.get::<_, Option<i64>>(12)?,
                            row.get::<_, Option<i64>>(13)?,
                            row.get::<_, Option<String>>(14)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            storage_attestation_predecessor_from_values(
                row,
                enrollment_epoch,
                process_instance_id,
                now_ms,
            )
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn
                .query_opt(
                    "SELECT v.current_epoch, v.enrollment_generation, v.status, \
                            v.active_instance_id, v.instance_lease_expires_at_ms, \
                            v.required_tombstone_generation, \
                            v.reconciled_tombstone_generation, k.key_fingerprint, \
                            a.attestation_generation, a.attestation_sha256, \
                            a.process_instance_id, a.enrollment_generation, \
                            a.required_tombstone_generation, \
                            a.reconciled_tombstone_generation, a.volume_key_fingerprint \
                       FROM jobs_runner_volumes v \
                       JOIN jobs_runner_volume_keys k \
                         ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                       LEFT JOIN jobs_runner_volume_storage_attestations a \
                         ON a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                        AND NOT EXISTS ( \
                          SELECT 1 FROM jobs_runner_volume_storage_attestations newer \
                           WHERE newer.volume_id = a.volume_id \
                             AND newer.enrollment_epoch = a.enrollment_epoch \
                             AND newer.attestation_generation > a.attestation_generation) \
                      WHERE v.volume_id = $1",
                    &[&volume_id],
                )?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            storage_attestation_predecessor_from_values(
                (
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
                ),
                enrollment_epoch,
                process_instance_id,
                now_ms,
            )
        }
    })
}

type StorageAttestationPredecessorValues = (
    i64,
    i64,
    String,
    Option<String>,
    Option<i64>,
    i64,
    i64,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

fn storage_attestation_predecessor_from_values(
    values: StorageAttestationPredecessorValues,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestationPredecessor> {
    if values.0 != enrollment_epoch
        || values.2 == "destroyed"
        || values.3.as_deref() != Some(process_instance_id)
        || values.4.is_none_or(|expiry| expiry <= now_ms)
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    let generation = values.8.unwrap_or(0);
    let sha256 = values
        .9
        .clone()
        .unwrap_or_else(|| GENESIS_RUNNER_STORAGE_ATTESTATION_SHA256.to_string());
    let current = values.8.is_some()
        && values.10.as_deref() == Some(process_instance_id)
        && values.11 == Some(values.1)
        && values.12 == Some(values.5)
        && values.13 == Some(values.6)
        && values.14.as_deref() == Some(values.7.as_str())
        && values.5 == values.6;
    Ok(RunnerVolumeStorageAttestationPredecessor {
        enrollment_generation: values.1,
        predecessor_attestation_generation: generation,
        predecessor_attestation_sha256: sha256,
        storage_attestation_required: !current,
    })
}

type LockedStorageAttestationVolume = (
    i64,
    i64,
    String,
    String,
    Option<String>,
    Option<i64>,
    i64,
    i64,
    String,
    String,
);

fn validate_storage_attestation_live_binding(
    attestation: &RunnerVolumeStorageAttestation,
    volume: &LockedStorageAttestationVolume,
    minimum_runner_build_id: &str,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    if received_at_ms < 0
        || attestation.observed_at_ms
            > received_at_ms.saturating_add(RUNNER_STORAGE_ATTESTATION_MAX_FUTURE_MS)
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    if volume.0 != attestation.enrollment_epoch
        || volume.1 != attestation.enrollment_generation
        || volume.2 != attestation.resource_fingerprint
        || volume.3 == "destroyed"
        || volume.4.as_deref() != Some(attestation.process_instance_id.as_str())
        || volume.5.is_none_or(|expiry| expiry <= received_at_ms)
        || volume.6 != attestation.required_tombstone_generation
        || volume.7 != attestation.reconciled_tombstone_generation
        || volume.6 != volume.7
        || volume.8 != attestation.volume_key_fingerprint
        || !runner_build_satisfies(&attestation.runner_build_id, minimum_runner_build_id)
    {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    verify_runner_volume_storage_attestation(attestation, &volume.9)
}

pub fn record_runner_volume_storage_attestation(
    pool: &DbPool,
    attestation: &RunnerVolumeStorageAttestation,
    minimum_runner_build_id: &str,
    received_at_ms: i64,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestationOutcome> {
    attestation.validate_unsigned()?;
    if !runner_build_id_is_canonical(minimum_runner_build_id) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    // Verify before opening the write transaction. The current key binding is
    // checked again under the fleet/volume locks below.
    verify_runner_volume_storage_attestation(attestation, &authority.volume.public_key_base64url)?;
    let attestation_sha256 = attestation.attestation_sha256()?;
    let canonical_unsigned_sha256 = attestation.canonical_unsigned_sha256()?;
    let canonical_json = serde_json::to_string(attestation).map_err(anyhow::Error::from)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_runner_volume_storage_attestation_sqlite(
            pool,
            attestation,
            minimum_runner_build_id,
            received_at_ms,
            authority,
            &attestation_sha256,
            &canonical_unsigned_sha256,
            &canonical_json,
        ),
        DbPool::Postgres(_) => record_runner_volume_storage_attestation_postgres(
            pool,
            attestation,
            minimum_runner_build_id,
            received_at_ms,
            authority,
            &attestation_sha256,
            &canonical_unsigned_sha256,
            &canonical_json,
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn record_runner_volume_storage_attestation_sqlite(
    pool: &DbPool,
    attestation: &RunnerVolumeStorageAttestation,
    minimum_runner_build_id: &str,
    received_at_ms: i64,
    authority: &VerifiedRunnerVolumeAuthority,
    attestation_sha256: &str,
    canonical_unsigned_sha256: &str,
    canonical_json: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestationOutcome> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let fleet_generation: i64 = tx.query_row(
        "SELECT storage_attestation_generation \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| row.get(0),
    )?;
    let volume = tx
        .query_row(
            "SELECT v.current_epoch, v.enrollment_generation, v.resource_fingerprint, \
                    v.status, v.active_instance_id, v.instance_lease_expires_at_ms, \
                    v.required_tombstone_generation, v.reconciled_tombstone_generation, \
                    k.key_fingerprint, k.public_key_base64url \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = ?1",
            params![attestation.volume_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    validate_storage_attestation_live_binding(
        attestation,
        &volume,
        minimum_runner_build_id,
        received_at_ms,
    )?;
    let existing = tx
        .query_row(
            "SELECT attestation_generation, fleet_attestation_generation, \
                    attestation_sha256, canonical_json \
               FROM jobs_runner_volume_storage_attestations \
              WHERE volume_id = ?1 AND enrollment_epoch = ?2 AND attestation_id = ?3",
            params![
                attestation.volume_id,
                attestation.enrollment_epoch,
                attestation.attestation_id,
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.2 != attestation_sha256 || existing.3 != canonical_json {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        consume_runner_volume_authority_sqlite_tx(
            &tx,
            authority,
            "storage_attestation",
            &attestation.volume_id,
            attestation.enrollment_epoch,
            &attestation.process_instance_id,
            received_at_ms,
        )?;
        tx.commit()?;
        return Ok(RunnerVolumeStorageAttestationOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            attestation_generation: existing.0,
            fleet_attestation_generation: existing.1,
            attestation_sha256: existing.2,
            volume_status: volume.3,
        });
    }
    let predecessor = tx
        .query_row(
            "SELECT attestation_generation, attestation_sha256 \
               FROM jobs_runner_volume_storage_attestations \
              WHERE volume_id = ?1 AND enrollment_epoch = ?2 \
              ORDER BY attestation_generation DESC LIMIT 1",
            params![attestation.volume_id, attestation.enrollment_epoch],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .unwrap_or((0, GENESIS_RUNNER_STORAGE_ATTESTATION_SHA256.to_string()));
    if predecessor.0 != attestation.predecessor_attestation_generation
        || predecessor.1 != attestation.predecessor_attestation_sha256
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let generation = predecessor
        .0
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let next_fleet_generation = fleet_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    consume_runner_volume_authority_sqlite_tx(
        &tx,
        authority,
        "storage_attestation",
        &attestation.volume_id,
        attestation.enrollment_epoch,
        &attestation.process_instance_id,
        received_at_ms,
    )?;
    insert_sqlite_runner_storage_attestation(
        &tx,
        attestation,
        generation,
        next_fleet_generation,
        attestation_sha256,
        canonical_unsigned_sha256,
        canonical_json,
        received_at_ms,
    )?;
    let (count, set_sha256) = sqlite_runner_storage_attestation_set(&tx)?;
    invalidate_sqlite_runner_fleet_cutover(&tx, received_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET storage_attestation_generation = ?1, storage_attestation_count = ?2, \
                storage_attestation_set_sha256 = ?3, updated_at_ms = ?4 \
          WHERE singleton_id = 1 AND storage_attestation_generation = ?5",
        params![
            next_fleet_generation,
            count,
            set_sha256,
            received_at_ms,
            fleet_generation,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    if tx.execute(
        "UPDATE jobs_runner_volumes \
            SET status = CASE WHEN status = 'reconciling' THEN 'active' ELSE status END, \
                updated_at_ms = ?2 \
          WHERE volume_id = ?1 AND current_epoch = ?3 AND active_instance_id = ?4 \
            AND instance_lease_expires_at_ms > ?2 \
            AND required_tombstone_generation = reconciled_tombstone_generation",
        params![
            attestation.volume_id,
            received_at_ms,
            attestation.enrollment_epoch,
            attestation.process_instance_id,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let volume_status: String = tx.query_row(
        "SELECT status FROM jobs_runner_volumes WHERE volume_id = ?1",
        params![attestation.volume_id],
        |row| row.get(0),
    )?;
    tx.commit()?;
    Ok(RunnerVolumeStorageAttestationOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        attestation_generation: generation,
        fleet_attestation_generation: next_fleet_generation,
        attestation_sha256: attestation_sha256.to_string(),
        volume_status,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_sqlite_runner_storage_attestation(
    tx: &RunnerSqliteTransaction<'_>,
    attestation: &RunnerVolumeStorageAttestation,
    generation: i64,
    fleet_generation: i64,
    attestation_sha256: &str,
    canonical_unsigned_sha256: &str,
    canonical_json: &str,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "INSERT INTO jobs_runner_volume_storage_attestations ( \
            volume_id, enrollment_epoch, volume_key_fingerprint, \
            attestation_generation, fleet_attestation_generation, version, audience, \
            attestation_id, resource_fingerprint, enrollment_generation, \
            process_instance_id, predecessor_attestation_generation, \
            predecessor_attestation_sha256, required_tombstone_generation, \
            reconciled_tombstone_generation, storage_evidence_version, \
            subject_storage_layout_version, root_device_id, root_link_count, \
            root_entry_count, root_file_bytes, root_sha256, subject_storage_subject_count, \
            subject_storage_subject_set_sha256, subject_storage_scope_count, \
            subject_storage_complete_root_entry_count, \
            subject_storage_complete_root_file_bytes, \
            subject_storage_complete_root_sha256, locator_count, resident_locator_count, \
            locator_set_sha256, legacy_inventory_version, legacy_artifact_count, \
            legacy_artifact_bytes, legacy_artifact_set_sha256, unclassified_root_count, \
            runner_build_id, observed_at_ms, signature, canonical_unsigned_sha256, \
            attestation_sha256, canonical_json, received_at_ms \
         ) VALUES ( \
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
            ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, \
            ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43 \
         )",
        params![
            attestation.volume_id,
            attestation.enrollment_epoch,
            attestation.volume_key_fingerprint,
            generation,
            fleet_generation,
            attestation.version,
            attestation.audience,
            attestation.attestation_id,
            attestation.resource_fingerprint,
            attestation.enrollment_generation,
            attestation.process_instance_id,
            attestation.predecessor_attestation_generation,
            attestation.predecessor_attestation_sha256,
            attestation.required_tombstone_generation,
            attestation.reconciled_tombstone_generation,
            attestation.storage_evidence_version,
            attestation.subject_storage_layout_version,
            attestation.root_device_id,
            attestation.root_link_count,
            attestation.root_entry_count,
            attestation.root_file_bytes,
            attestation.root_sha256,
            attestation.subject_storage_subject_count,
            attestation.subject_storage_subject_set_sha256,
            attestation.subject_storage_scope_count,
            attestation.subject_storage_complete_root_entry_count,
            attestation.subject_storage_complete_root_file_bytes,
            attestation.subject_storage_complete_root_sha256,
            attestation.locator_count,
            attestation.resident_locator_count,
            attestation.locator_set_sha256,
            attestation.legacy_inventory_version,
            attestation.legacy_artifact_count,
            attestation.legacy_artifact_bytes,
            attestation.legacy_artifact_set_sha256,
            attestation.unclassified_root_count,
            attestation.runner_build_id,
            attestation.observed_at_ms,
            attestation.signature,
            canonical_unsigned_sha256,
            attestation_sha256,
            canonical_json,
            received_at_ms,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_runner_volume_storage_attestation_postgres(
    pool: &DbPool,
    attestation: &RunnerVolumeStorageAttestation,
    minimum_runner_build_id: &str,
    received_at_ms: i64,
    authority: &VerifiedRunnerVolumeAuthority,
    attestation_sha256: &str,
    canonical_unsigned_sha256: &str,
    canonical_json: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeStorageAttestationOutcome> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let fleet_generation: i64 = tx
        .query_one(
            "SELECT storage_attestation_generation \
               FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE",
            &[],
        )?
        .get(0);
    let row = tx
        .query_opt(
            "SELECT v.current_epoch, v.enrollment_generation, v.resource_fingerprint, \
                    v.status, v.active_instance_id, v.instance_lease_expires_at_ms, \
                    v.required_tombstone_generation, v.reconciled_tombstone_generation, \
                    k.key_fingerprint, k.public_key_base64url \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = $1 FOR UPDATE OF v, k",
            &[&attestation.volume_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let volume = (
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
    );
    validate_storage_attestation_live_binding(
        attestation,
        &volume,
        minimum_runner_build_id,
        received_at_ms,
    )?;
    if let Some(row) = tx.query_opt(
        "SELECT attestation_generation, fleet_attestation_generation, \
                attestation_sha256, canonical_json \
           FROM jobs_runner_volume_storage_attestations \
          WHERE volume_id = $1 AND enrollment_epoch = $2 AND attestation_id = $3 \
          FOR UPDATE",
        &[
            &attestation.volume_id,
            &attestation.enrollment_epoch,
            &attestation.attestation_id,
        ],
    )? {
        let existing_sha256: String = row.get(2);
        let existing_json: String = row.get(3);
        if existing_sha256 != attestation_sha256 || existing_json != canonical_json {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        consume_runner_volume_authority_postgres_tx(
            &mut tx,
            authority,
            "storage_attestation",
            &attestation.volume_id,
            attestation.enrollment_epoch,
            &attestation.process_instance_id,
            received_at_ms,
        )?;
        let outcome = RunnerVolumeStorageAttestationOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            attestation_generation: row.get(0),
            fleet_attestation_generation: row.get(1),
            attestation_sha256: existing_sha256,
            volume_status: volume.3,
        };
        tx.commit()?;
        return Ok(outcome);
    }
    let predecessor = tx
        .query_opt(
            "SELECT attestation_generation, attestation_sha256 \
               FROM jobs_runner_volume_storage_attestations \
              WHERE volume_id = $1 AND enrollment_epoch = $2 \
              ORDER BY attestation_generation DESC LIMIT 1 FOR UPDATE",
            &[&attestation.volume_id, &attestation.enrollment_epoch],
        )?
        .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)))
        .unwrap_or((0, GENESIS_RUNNER_STORAGE_ATTESTATION_SHA256.to_string()));
    if predecessor.0 != attestation.predecessor_attestation_generation
        || predecessor.1 != attestation.predecessor_attestation_sha256
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let generation = predecessor
        .0
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let next_fleet_generation = fleet_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    consume_runner_volume_authority_postgres_tx(
        &mut tx,
        authority,
        "storage_attestation",
        &attestation.volume_id,
        attestation.enrollment_epoch,
        &attestation.process_instance_id,
        received_at_ms,
    )?;
    insert_postgres_runner_storage_attestation(
        &mut tx,
        attestation,
        generation,
        next_fleet_generation,
        attestation_sha256,
        canonical_unsigned_sha256,
        canonical_json,
        received_at_ms,
    )?;
    let (count, set_sha256) = postgres_runner_storage_attestation_set(&mut tx)?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, received_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET storage_attestation_generation = $1, storage_attestation_count = $2, \
                storage_attestation_set_sha256 = $3, updated_at_ms = $4 \
          WHERE singleton_id = 1 AND storage_attestation_generation = $5",
        &[
            &next_fleet_generation,
            &count,
            &set_sha256,
            &received_at_ms,
            &fleet_generation,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    if tx.execute(
        "UPDATE jobs_runner_volumes \
            SET status = CASE WHEN status = 'reconciling' THEN 'active' ELSE status END, \
                updated_at_ms = $2 \
          WHERE volume_id = $1 AND current_epoch = $3 AND active_instance_id = $4 \
            AND instance_lease_expires_at_ms > $2 \
            AND required_tombstone_generation = reconciled_tombstone_generation",
        &[
            &attestation.volume_id,
            &received_at_ms,
            &attestation.enrollment_epoch,
            &attestation.process_instance_id,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let volume_status: String = tx
        .query_one(
            "SELECT status FROM jobs_runner_volumes WHERE volume_id = $1",
            &[&attestation.volume_id],
        )?
        .get(0);
    tx.commit()?;
    Ok(RunnerVolumeStorageAttestationOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        attestation_generation: generation,
        fleet_attestation_generation: next_fleet_generation,
        attestation_sha256: attestation_sha256.to_string(),
        volume_status,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_postgres_runner_storage_attestation(
    tx: &mut postgres::Transaction<'_>,
    attestation: &RunnerVolumeStorageAttestation,
    generation: i64,
    fleet_generation: i64,
    attestation_sha256: &str,
    canonical_unsigned_sha256: &str,
    canonical_json: &str,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "INSERT INTO jobs_runner_volume_storage_attestations ( \
            volume_id, enrollment_epoch, volume_key_fingerprint, \
            attestation_generation, fleet_attestation_generation, version, audience, \
            attestation_id, resource_fingerprint, enrollment_generation, \
            process_instance_id, predecessor_attestation_generation, \
            predecessor_attestation_sha256, required_tombstone_generation, \
            reconciled_tombstone_generation, storage_evidence_version, \
            subject_storage_layout_version, root_device_id, root_link_count, \
            root_entry_count, root_file_bytes, root_sha256, subject_storage_subject_count, \
            subject_storage_subject_set_sha256, subject_storage_scope_count, \
            subject_storage_complete_root_entry_count, \
            subject_storage_complete_root_file_bytes, \
            subject_storage_complete_root_sha256, locator_count, resident_locator_count, \
            locator_set_sha256, legacy_inventory_version, legacy_artifact_count, \
            legacy_artifact_bytes, legacy_artifact_set_sha256, unclassified_root_count, \
            runner_build_id, observed_at_ms, signature, canonical_unsigned_sha256, \
            attestation_sha256, canonical_json, received_at_ms \
         ) VALUES ( \
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
            $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, \
            $30, $31, $32, $33, $34, $35, $36, $37, $38, $39, $40, $41, $42, $43 \
         )",
        &[
            &attestation.volume_id,
            &attestation.enrollment_epoch,
            &attestation.volume_key_fingerprint,
            &generation,
            &fleet_generation,
            &attestation.version,
            &attestation.audience,
            &attestation.attestation_id,
            &attestation.resource_fingerprint,
            &attestation.enrollment_generation,
            &attestation.process_instance_id,
            &attestation.predecessor_attestation_generation,
            &attestation.predecessor_attestation_sha256,
            &attestation.required_tombstone_generation,
            &attestation.reconciled_tombstone_generation,
            &attestation.storage_evidence_version,
            &attestation.subject_storage_layout_version,
            &attestation.root_device_id,
            &attestation.root_link_count,
            &attestation.root_entry_count,
            &attestation.root_file_bytes,
            &attestation.root_sha256,
            &attestation.subject_storage_subject_count,
            &attestation.subject_storage_subject_set_sha256,
            &attestation.subject_storage_scope_count,
            &attestation.subject_storage_complete_root_entry_count,
            &attestation.subject_storage_complete_root_file_bytes,
            &attestation.subject_storage_complete_root_sha256,
            &attestation.locator_count,
            &attestation.resident_locator_count,
            &attestation.locator_set_sha256,
            &attestation.legacy_inventory_version,
            &attestation.legacy_artifact_count,
            &attestation.legacy_artifact_bytes,
            &attestation.legacy_artifact_set_sha256,
            &attestation.unclassified_root_count,
            &attestation.runner_build_id,
            &attestation.observed_at_ms,
            &attestation.signature,
            &canonical_unsigned_sha256,
            &attestation_sha256,
            &canonical_json,
            &received_at_ms,
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeFleetStatus {
    pub enrollment_generation: i64,
    pub purge_generation: i64,
    pub tombstone_generation: i64,
    pub destruction_generation: i64,
    pub legacy_reconciliation_generation: i64,
    pub storage_attestation_generation: i64,
    pub storage_attestation_count: i64,
    pub storage_attestation_set_sha256: String,
    pub legacy_inventory_state: String,
    pub legacy_inventory_generation: i64,
    pub legacy_inventory_reconciliation_id: Option<String>,
    pub legacy_inventory_authority_id: Option<String>,
    pub legacy_inventory_authority_sha256: Option<String>,
    pub legacy_inventory_root_count: Option<i64>,
    pub legacy_inventory_root_set_sha256: Option<String>,
    pub cutover_state: String,
    pub unresolved_legacy_volume_count: i64,
    pub cutover_enrollment_generation: Option<i64>,
    pub cutover_purge_generation: Option<i64>,
    pub cutover_tombstone_generation: Option<i64>,
    pub cutover_destruction_generation: Option<i64>,
    pub cutover_legacy_reconciliation_generation: Option<i64>,
    pub cutover_storage_attestation_generation: Option<i64>,
    pub cutover_storage_attestation_count: Option<i64>,
    pub cutover_storage_attestation_set_sha256: Option<String>,
    pub cutover_legacy_inventory_generation: Option<i64>,
    pub cutover_legacy_inventory_reconciliation_id: Option<String>,
    pub cutover_legacy_inventory_authority_id: Option<String>,
    pub cutover_legacy_inventory_authority_sha256: Option<String>,
    pub cutover_legacy_inventory_root_count: Option<i64>,
    pub cutover_legacy_inventory_root_set_sha256: Option<String>,
    pub cutover_non_destroyed_volume_count: Option<i64>,
    pub cutover_destruction_count: Option<i64>,
    pub cutover_unresolved_legacy_volume_count: Option<i64>,
    pub cutover_evidence_ref: Option<String>,
    pub cutover_evidence_sha256: Option<String>,
    pub cutover_authorized_by: Option<String>,
    pub cutover_at_ms: Option<i64>,
    pub non_destroyed_volume_count: i64,
    pub destruction_count: i64,
    pub attested_reconciled_volume_count: i64,
    pub updated_at_ms: i64,
}

pub fn runner_volume_fleet_status(
    pool: &DbPool,
) -> RunnerVolumePurgeResult<RunnerVolumeFleetStatus> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT f.enrollment_generation, f.purge_generation, \
                        f.tombstone_generation, f.destruction_generation, \
                        f.legacy_reconciliation_generation, f.legacy_inventory_state, \
                        f.legacy_inventory_generation, f.legacy_inventory_reconciliation_id, \
                        f.legacy_inventory_authority_id, \
                        f.legacy_inventory_authority_sha256, f.legacy_inventory_root_count, \
                        f.legacy_inventory_root_set_sha256, f.cutover_state, \
                        f.unresolved_legacy_volume_count, f.cutover_enrollment_generation, \
                        f.cutover_purge_generation, f.cutover_tombstone_generation, \
                        f.cutover_destruction_generation, \
                        f.cutover_legacy_reconciliation_generation, \
                        f.cutover_legacy_inventory_generation, \
                        f.cutover_legacy_inventory_reconciliation_id, \
                        f.cutover_legacy_inventory_authority_id, \
                        f.cutover_legacy_inventory_authority_sha256, \
                        f.cutover_legacy_inventory_root_count, \
                        f.cutover_legacy_inventory_root_set_sha256, \
                        f.cutover_non_destroyed_volume_count, f.cutover_destruction_count, \
                        f.cutover_unresolved_legacy_volume_count, f.cutover_evidence_ref, \
                        f.cutover_evidence_sha256, f.cutover_authorized_by, f.cutover_at_ms, \
                        (SELECT COUNT(*) FROM jobs_runner_volumes v \
                          WHERE v.status <> 'destroyed'), \
                        (SELECT COUNT(*) FROM jobs_runner_volume_destructions), \
                        (SELECT COUNT(*) FROM jobs_runner_volumes v \
                          WHERE v.status IN ('active', 'suspended', 'retired') \
                          AND v.required_tombstone_generation = \
                              v.reconciled_tombstone_generation \
                          AND EXISTS (SELECT 1 \
                            FROM jobs_runner_volume_storage_attestations a \
                           WHERE a.volume_id = v.volume_id \
                             AND a.enrollment_epoch = v.current_epoch \
                             AND a.process_instance_id = v.active_instance_id \
                             AND a.enrollment_generation = v.enrollment_generation \
                             AND a.required_tombstone_generation = \
                                 v.required_tombstone_generation \
                             AND a.reconciled_tombstone_generation = \
                                 v.reconciled_tombstone_generation \
                             AND NOT EXISTS (SELECT 1 \
                               FROM jobs_runner_volume_storage_attestations newer \
                              WHERE newer.volume_id = a.volume_id \
                                AND newer.enrollment_epoch = a.enrollment_epoch \
                                AND newer.attestation_generation > \
                                    a.attestation_generation))), \
                        f.storage_attestation_generation, f.storage_attestation_count, \
                        f.storage_attestation_set_sha256, \
                        f.cutover_storage_attestation_generation, \
                        f.cutover_storage_attestation_count, \
                        f.cutover_storage_attestation_set_sha256, \
                        f.updated_at_ms \
                   FROM jobs_runner_volume_fleet_state f WHERE f.singleton_id = 1",
                [],
                |row| {
                    Ok(RunnerVolumeFleetStatus {
                        enrollment_generation: row.get(0)?,
                        purge_generation: row.get(1)?,
                        tombstone_generation: row.get(2)?,
                        destruction_generation: row.get(3)?,
                        legacy_reconciliation_generation: row.get(4)?,
                        storage_attestation_generation: row.get(35)?,
                        storage_attestation_count: row.get(36)?,
                        storage_attestation_set_sha256: row.get(37)?,
                        legacy_inventory_state: row.get(5)?,
                        legacy_inventory_generation: row.get(6)?,
                        legacy_inventory_reconciliation_id: row.get(7)?,
                        legacy_inventory_authority_id: row.get(8)?,
                        legacy_inventory_authority_sha256: row.get(9)?,
                        legacy_inventory_root_count: row.get(10)?,
                        legacy_inventory_root_set_sha256: row.get(11)?,
                        cutover_state: row.get(12)?,
                        unresolved_legacy_volume_count: row.get(13)?,
                        cutover_enrollment_generation: row.get(14)?,
                        cutover_purge_generation: row.get(15)?,
                        cutover_tombstone_generation: row.get(16)?,
                        cutover_destruction_generation: row.get(17)?,
                        cutover_legacy_reconciliation_generation: row.get(18)?,
                        cutover_storage_attestation_generation: row.get(38)?,
                        cutover_storage_attestation_count: row.get(39)?,
                        cutover_storage_attestation_set_sha256: row.get(40)?,
                        cutover_legacy_inventory_generation: row.get(19)?,
                        cutover_legacy_inventory_reconciliation_id: row.get(20)?,
                        cutover_legacy_inventory_authority_id: row.get(21)?,
                        cutover_legacy_inventory_authority_sha256: row.get(22)?,
                        cutover_legacy_inventory_root_count: row.get(23)?,
                        cutover_legacy_inventory_root_set_sha256: row.get(24)?,
                        cutover_non_destroyed_volume_count: row.get(25)?,
                        cutover_destruction_count: row.get(26)?,
                        cutover_unresolved_legacy_volume_count: row.get(27)?,
                        cutover_evidence_ref: row.get(28)?,
                        cutover_evidence_sha256: row.get(29)?,
                        cutover_authorized_by: row.get(30)?,
                        cutover_at_ms: row.get(31)?,
                        non_destroyed_volume_count: row.get(32)?,
                        destruction_count: row.get(33)?,
                        attested_reconciled_volume_count: row.get(34)?,
                        updated_at_ms: row.get(41)?,
                    })
                },
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_one(
                "SELECT f.enrollment_generation, f.purge_generation, \
                        f.tombstone_generation, f.destruction_generation, \
                        f.legacy_reconciliation_generation, f.legacy_inventory_state, \
                        f.legacy_inventory_generation, f.legacy_inventory_reconciliation_id, \
                        f.legacy_inventory_authority_id, \
                        f.legacy_inventory_authority_sha256, f.legacy_inventory_root_count, \
                        f.legacy_inventory_root_set_sha256, f.cutover_state, \
                        f.unresolved_legacy_volume_count, f.cutover_enrollment_generation, \
                        f.cutover_purge_generation, f.cutover_tombstone_generation, \
                        f.cutover_destruction_generation, \
                        f.cutover_legacy_reconciliation_generation, \
                        f.cutover_legacy_inventory_generation, \
                        f.cutover_legacy_inventory_reconciliation_id, \
                        f.cutover_legacy_inventory_authority_id, \
                        f.cutover_legacy_inventory_authority_sha256, \
                        f.cutover_legacy_inventory_root_count, \
                        f.cutover_legacy_inventory_root_set_sha256, \
                        f.cutover_non_destroyed_volume_count, f.cutover_destruction_count, \
                        f.cutover_unresolved_legacy_volume_count, f.cutover_evidence_ref, \
                        f.cutover_evidence_sha256, f.cutover_authorized_by, f.cutover_at_ms, \
                        (SELECT COUNT(*) FROM jobs_runner_volumes v \
                          WHERE v.status <> 'destroyed'), \
                        (SELECT COUNT(*) FROM jobs_runner_volume_destructions), \
                        (SELECT COUNT(*) FROM jobs_runner_volumes v \
                          WHERE v.status IN ('active', 'suspended', 'retired') \
                          AND v.required_tombstone_generation = \
                              v.reconciled_tombstone_generation \
                          AND EXISTS (SELECT 1 \
                            FROM jobs_runner_volume_storage_attestations a \
                           WHERE a.volume_id = v.volume_id \
                             AND a.enrollment_epoch = v.current_epoch \
                             AND a.process_instance_id = v.active_instance_id \
                             AND a.enrollment_generation = v.enrollment_generation \
                             AND a.required_tombstone_generation = \
                                 v.required_tombstone_generation \
                             AND a.reconciled_tombstone_generation = \
                                 v.reconciled_tombstone_generation \
                             AND NOT EXISTS (SELECT 1 \
                               FROM jobs_runner_volume_storage_attestations newer \
                              WHERE newer.volume_id = a.volume_id \
                                AND newer.enrollment_epoch = a.enrollment_epoch \
                                AND newer.attestation_generation > \
                                    a.attestation_generation))), \
                        f.storage_attestation_generation, f.storage_attestation_count, \
                        f.storage_attestation_set_sha256, \
                        f.cutover_storage_attestation_generation, \
                        f.cutover_storage_attestation_count, \
                        f.cutover_storage_attestation_set_sha256, \
                        f.updated_at_ms \
                   FROM jobs_runner_volume_fleet_state f WHERE f.singleton_id = 1",
                &[],
            )?;
            Ok(RunnerVolumeFleetStatus {
                enrollment_generation: row.get(0),
                purge_generation: row.get(1),
                tombstone_generation: row.get(2),
                destruction_generation: row.get(3),
                legacy_reconciliation_generation: row.get(4),
                storage_attestation_generation: row.get(35),
                storage_attestation_count: row.get(36),
                storage_attestation_set_sha256: row.get(37),
                legacy_inventory_state: row.get(5),
                legacy_inventory_generation: row.get(6),
                legacy_inventory_reconciliation_id: row.get(7),
                legacy_inventory_authority_id: row.get(8),
                legacy_inventory_authority_sha256: row.get(9),
                legacy_inventory_root_count: row.get(10),
                legacy_inventory_root_set_sha256: row.get(11),
                cutover_state: row.get(12),
                unresolved_legacy_volume_count: row.get(13),
                cutover_enrollment_generation: row.get(14),
                cutover_purge_generation: row.get(15),
                cutover_tombstone_generation: row.get(16),
                cutover_destruction_generation: row.get(17),
                cutover_legacy_reconciliation_generation: row.get(18),
                cutover_storage_attestation_generation: row.get(38),
                cutover_storage_attestation_count: row.get(39),
                cutover_storage_attestation_set_sha256: row.get(40),
                cutover_legacy_inventory_generation: row.get(19),
                cutover_legacy_inventory_reconciliation_id: row.get(20),
                cutover_legacy_inventory_authority_id: row.get(21),
                cutover_legacy_inventory_authority_sha256: row.get(22),
                cutover_legacy_inventory_root_count: row.get(23),
                cutover_legacy_inventory_root_set_sha256: row.get(24),
                cutover_non_destroyed_volume_count: row.get(25),
                cutover_destruction_count: row.get(26),
                cutover_unresolved_legacy_volume_count: row.get(27),
                cutover_evidence_ref: row.get(28),
                cutover_evidence_sha256: row.get(29),
                cutover_authorized_by: row.get(30),
                cutover_at_ms: row.get(31),
                non_destroyed_volume_count: row.get(32),
                destruction_count: row.get(33),
                attested_reconciled_volume_count: row.get(34),
                updated_at_ms: row.get(41),
            })
        }
    })
}

fn exact_enrolled_volume(stored: &RunnerVolumeRecord, input: &EnrollRunnerVolumeRequest) -> bool {
    let proof = &input.proof;
    stored.volume_id == proof.volume_id
        && stored.worker_id == proof.worker_id
        && stored.provider == proof.provider
        && stored.provider_resource_id == proof.provider_resource_id
        && stored.resource_fingerprint == proof.resource_fingerprint
        && stored.current_epoch == proof.enrollment_epoch
        && stored.admission_grant_id == proof.admission_grant_id
        && stored.legacy_artifact_count == proof.legacy_artifact_count
        && stored.public_key_base64url == proof.public_key_base64url
        && stored.key_fingerprint == proof.key_fingerprint
}

pub fn enroll_runner_volume(
    pool: &DbPool,
    input: &EnrollRunnerVolumeRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeRecord> {
    validate_enrollment_request(input)?;
    let token_sha256 = hex::encode(Sha256::digest(decode_base64url_exact(
        &input.grant_token,
        32,
    )?));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => enroll_runner_volume_sqlite(pool, input, &token_sha256),
        DbPool::Postgres(_) => enroll_runner_volume_postgres(pool, input, &token_sha256),
    })
}

fn enroll_runner_volume_sqlite(
    pool: &DbPool,
    input: &EnrollRunnerVolumeRequest,
    token_sha256: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeRecord> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (fleet_generation, tombstone_generation): (i64, i64) = tx.query_row(
        "SELECT enrollment_generation, tombstone_generation \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let select_volume = format!(
        "SELECT {RUNNER_VOLUME_COLUMNS} \
           FROM jobs_runner_volumes v \
           JOIN jobs_runner_volume_keys k \
             ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
          WHERE v.volume_id = ?1"
    );
    if let Some(stored) = tx
        .query_row(
            &select_volume,
            params![input.proof.volume_id],
            runner_volume_from_sqlite_row,
        )
        .optional()?
    {
        if !exact_enrolled_volume(&stored, input) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(stored);
    }
    let grant_select = format!(
        "SELECT {ADMISSION_GRANT_COLUMNS} \
           FROM jobs_runner_volume_admission_grants WHERE grant_id = ?1"
    );
    let grant = tx
        .query_row(
            &grant_select,
            params![input.proof.admission_grant_id],
            admission_grant_from_sqlite_row,
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::Unauthorized)?;
    validate_grant_for_enrollment(&grant, input, token_sha256)?;
    if tx.query_row(
        "SELECT EXISTS( \
             SELECT 1 FROM jobs_runner_volumes \
              WHERE provider = ?1 AND provider_resource_id = ?2 \
                 OR resource_fingerprint = ?3 \
         ) OR EXISTS( \
             SELECT 1 FROM jobs_runner_volume_keys WHERE key_fingerprint = ?4 \
         )",
        params![
            input.proof.provider,
            input.proof.provider_resource_id,
            input.proof.resource_fingerprint,
            input.proof.key_fingerprint,
        ],
        |row| row.get::<_, i64>(0),
    )? != 0
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let enrollment_generation = fleet_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    insert_enrolled_runner_volume_sqlite(&tx, input, enrollment_generation, tombstone_generation)?;
    tx.execute(
        "UPDATE jobs_runner_volume_admission_grants \
            SET consumed_volume_id = ?2, consumed_at_ms = ?3 \
          WHERE grant_id = ?1 AND consumed_volume_id IS NULL",
        params![
            input.proof.admission_grant_id,
            input.proof.volume_id,
            input.enrolled_at_ms,
        ],
    )?;
    let (storage_attestation_count, storage_attestation_set_sha256) =
        sqlite_runner_storage_attestation_set(&tx)?;
    invalidate_sqlite_runner_fleet_cutover(&tx, input.enrolled_at_ms)?;
    tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET enrollment_generation = ?1, \
                legacy_inventory_state = CASE \
                  WHEN legacy_inventory_state = 'unknown' THEN 'unknown' ELSE 'reconciling' END, \
                storage_attestation_count = ?4, \
                storage_attestation_set_sha256 = ?5, updated_at_ms = ?2 \
          WHERE singleton_id = 1 AND enrollment_generation = ?3",
        params![
            enrollment_generation,
            input.enrolled_at_ms,
            fleet_generation,
            storage_attestation_count,
            storage_attestation_set_sha256,
        ],
    )?;
    let mut created = tx.query_row(
        &select_volume,
        params![input.proof.volume_id],
        runner_volume_from_sqlite_row,
    )?;
    created.disposition = RunnerVolumeWriteDisposition::Applied;
    tx.commit()?;
    Ok(created)
}

fn insert_enrolled_runner_volume_sqlite(
    tx: &RunnerSqliteTransaction<'_>,
    input: &EnrollRunnerVolumeRequest,
    enrollment_generation: i64,
    tombstone_generation: i64,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "INSERT INTO jobs_runner_volumes ( \
            volume_id, worker_id, provider, provider_resource_id, resource_fingerprint, \
            current_epoch, enrollment_generation, required_tombstone_generation, \
            reconciled_tombstone_generation, status, legacy_artifact_count, \
            admission_grant_id, enrolled_at_ms, last_seen_at_ms, updated_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 'reconciling', \
                   ?9, ?10, ?11, ?11, ?11)",
        params![
            input.proof.volume_id,
            input.proof.worker_id,
            input.proof.provider,
            input.proof.provider_resource_id,
            input.proof.resource_fingerprint,
            input.proof.enrollment_epoch,
            enrollment_generation,
            tombstone_generation,
            input.proof.legacy_artifact_count,
            input.proof.admission_grant_id,
            input.enrolled_at_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO jobs_runner_volume_keys ( \
            volume_id, enrollment_epoch, public_key_base64url, key_fingerprint, \
            activated_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            input.proof.volume_id,
            input.proof.enrollment_epoch,
            input.proof.public_key_base64url,
            input.proof.key_fingerprint,
            input.enrolled_at_ms,
        ],
    )?;
    Ok(())
}

fn enroll_runner_volume_postgres(
    pool: &DbPool,
    input: &EnrollRunnerVolumeRequest,
    token_sha256: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeRecord> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let fleet = tx.query_one(
        "SELECT enrollment_generation, tombstone_generation \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let fleet_generation: i64 = fleet.get(0);
    let tombstone_generation: i64 = fleet.get(1);
    let select_volume = format!(
        "SELECT {RUNNER_VOLUME_COLUMNS} \
           FROM jobs_runner_volumes v \
           JOIN jobs_runner_volume_keys k \
             ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
          WHERE v.volume_id = $1 FOR UPDATE OF v, k"
    );
    if let Some(row) = tx.query_opt(&select_volume, &[&input.proof.volume_id])? {
        let stored = runner_volume_from_pg_row(row);
        if !exact_enrolled_volume(&stored, input) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(stored);
    }
    let grant_select = format!(
        "SELECT {ADMISSION_GRANT_COLUMNS} \
           FROM jobs_runner_volume_admission_grants WHERE grant_id = $1 FOR UPDATE"
    );
    let grant = tx
        .query_opt(&grant_select, &[&input.proof.admission_grant_id])?
        .map(admission_grant_from_pg_row)
        .ok_or(RunnerVolumePurgeError::Unauthorized)?;
    validate_grant_for_enrollment(&grant, input, token_sha256)?;
    let conflict = tx.query_one(
        "SELECT EXISTS( \
             SELECT 1 FROM jobs_runner_volumes \
              WHERE (provider = $1 AND provider_resource_id = $2) \
                 OR resource_fingerprint = $3 \
         ) OR EXISTS( \
             SELECT 1 FROM jobs_runner_volume_keys WHERE key_fingerprint = $4 \
         )",
        &[
            &input.proof.provider,
            &input.proof.provider_resource_id,
            &input.proof.resource_fingerprint,
            &input.proof.key_fingerprint,
        ],
    )?;
    if conflict.get::<_, bool>(0) {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let enrollment_generation = fleet_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_volumes ( \
            volume_id, worker_id, provider, provider_resource_id, resource_fingerprint, \
            current_epoch, enrollment_generation, required_tombstone_generation, \
            reconciled_tombstone_generation, status, legacy_artifact_count, \
            admission_grant_id, enrolled_at_ms, last_seen_at_ms, updated_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, 'reconciling', \
                   $9, $10, $11, $11, $11)",
        &[
            &input.proof.volume_id,
            &input.proof.worker_id,
            &input.proof.provider,
            &input.proof.provider_resource_id,
            &input.proof.resource_fingerprint,
            &input.proof.enrollment_epoch,
            &enrollment_generation,
            &tombstone_generation,
            &input.proof.legacy_artifact_count,
            &input.proof.admission_grant_id,
            &input.enrolled_at_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO jobs_runner_volume_keys ( \
            volume_id, enrollment_epoch, public_key_base64url, key_fingerprint, activated_at_ms \
         ) VALUES ($1, $2, $3, $4, $5)",
        &[
            &input.proof.volume_id,
            &input.proof.enrollment_epoch,
            &input.proof.public_key_base64url,
            &input.proof.key_fingerprint,
            &input.enrolled_at_ms,
        ],
    )?;
    if tx.execute(
        "UPDATE jobs_runner_volume_admission_grants \
            SET consumed_volume_id = $2, consumed_at_ms = $3 \
          WHERE grant_id = $1 AND consumed_volume_id IS NULL",
        &[
            &input.proof.admission_grant_id,
            &input.proof.volume_id,
            &input.enrolled_at_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let (storage_attestation_count, storage_attestation_set_sha256) =
        postgres_runner_storage_attestation_set(&mut tx)?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, input.enrolled_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET enrollment_generation = $1, \
                legacy_inventory_state = CASE \
                  WHEN legacy_inventory_state = 'unknown' THEN 'unknown' ELSE 'reconciling' END, \
                storage_attestation_count = $4, \
                storage_attestation_set_sha256 = $5, updated_at_ms = $2 \
          WHERE singleton_id = 1 AND enrollment_generation = $3",
        &[
            &enrollment_generation,
            &input.enrolled_at_ms,
            &fleet_generation,
            &storage_attestation_count,
            &storage_attestation_set_sha256,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let mut created =
        runner_volume_from_pg_row(tx.query_one(&select_volume, &[&input.proof.volume_id])?);
    created.disposition = RunnerVolumeWriteDisposition::Applied;
    tx.commit()?;
    Ok(created)
}

fn validate_grant_for_enrollment(
    grant: &RunnerVolumeAdmissionGrant,
    input: &EnrollRunnerVolumeRequest,
    token_sha256: &str,
) -> RunnerVolumePurgeResult<()> {
    if grant.consumed_volume_id.is_some()
        || grant.consumed_at_ms.is_some()
        || grant.expires_at_ms < input.enrolled_at_ms
        || grant.expected_worker_id != input.proof.worker_id
        || grant.provider != input.proof.provider
        || grant.provider_resource_id != input.proof.provider_resource_id
        || grant.resource_fingerprint != input.proof.resource_fingerprint
        || grant
            .token_sha256
            .as_bytes()
            .ct_eq(token_sha256.as_bytes())
            .unwrap_u8()
            != 1
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub fn claim_runner_volume_instance(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    let request = RunnerVolumeInstanceLeaseRequest {
        volume_id,
        enrollment_epoch,
        process_instance_id,
        now_ms,
        lease_expires_at_ms,
    };
    claim_runner_volume_instance_inner(pool, &request, None, None)
}

pub fn claim_runner_volume_instance_authorized(
    pool: &DbPool,
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
    runtime_claim: &RunnerProcessRuntimeGrantClaim,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    claim_runner_volume_instance_inner(pool, request, Some(runtime_claim), Some(authority))
}

fn claim_runner_volume_instance_inner(
    pool: &DbPool,
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
    runtime_claim: Option<&RunnerProcessRuntimeGrantClaim>,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    validate_instance_lease_request(request)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            claim_runner_volume_instance_sqlite(pool, request, runtime_claim, authority)
        }
        DbPool::Postgres(_) => {
            claim_runner_volume_instance_postgres(pool, request, runtime_claim, authority)
        }
    })
}

fn validate_instance_lease_request(
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
) -> RunnerVolumePurgeResult<()> {
    require_base64url(request.volume_id, 32)?;
    require_base64url(request.process_instance_id, 32)?;
    if request.enrollment_epoch <= 0
        || request.now_ms < 0
        || request.lease_expires_at_ms <= request.now_ms
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn claim_runner_volume_instance_sqlite(
    pool: &DbPool,
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
    runtime_claim: Option<&RunnerProcessRuntimeGrantClaim>,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (worker_id, current_epoch, status, active_instance, active_expiry): (
        String,
        i64,
        String,
        Option<String>,
        Option<i64>,
    ) = tx
        .query_row(
            "SELECT worker_id, current_epoch, status, active_instance_id, \
                    instance_lease_expires_at_ms \
               FROM jobs_runner_volumes WHERE volume_id = ?1",
            params![request.volume_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    validate_instance_claim_state(
        current_epoch,
        &status,
        active_instance.as_deref(),
        active_expiry,
        request.enrollment_epoch,
        request.process_instance_id,
        request.now_ms,
    )?;
    let trusted_runtime = match runtime_claim {
        Some(claim) => Some(bind_runner_process_runtime_sqlite_tx(
            &tx,
            claim,
            &worker_id,
            request.volume_id,
            request.enrollment_epoch,
            request.process_instance_id,
            request.now_ms,
        )?),
        None => None,
    };
    if let Some(authority) = authority {
        consume_runner_volume_authority_sqlite_tx(
            &tx,
            authority,
            "instance_claim",
            request.volume_id,
            request.enrollment_epoch,
            request.process_instance_id,
            request.now_ms,
        )?;
    }
    let disposition = if active_instance.as_deref() == Some(request.process_instance_id) {
        RunnerVolumeWriteDisposition::Replay
    } else {
        RunnerVolumeWriteDisposition::Applied
    };
    if disposition == RunnerVolumeWriteDisposition::Applied {
        invalidate_sqlite_runner_fleet_cutover(&tx, request.now_ms)?;
    }
    if tx.execute(
        "UPDATE jobs_runner_volumes \
            SET active_instance_id = ?2, instance_lease_expires_at_ms = ?3, \
                status = CASE WHEN status = 'active' THEN 'reconciling' ELSE status END, \
                last_seen_at_ms = ?4, updated_at_ms = ?4 \
          WHERE volume_id = ?1 AND current_epoch = ?5",
        params![
            request.volume_id,
            request.process_instance_id,
            request.lease_expires_at_ms,
            request.now_ms,
            request.enrollment_epoch,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeInstanceLease {
        volume_id: request.volume_id.to_string(),
        enrollment_epoch: request.enrollment_epoch,
        process_instance_id: request.process_instance_id.to_string(),
        runtime_grant_id: trusted_runtime
            .as_ref()
            .map(|runtime| runtime.runtime_grant_id.clone()),
        runtime_sha256: trusted_runtime.map(|runtime| runtime.runtime_sha256),
        lease_expires_at_ms: request.lease_expires_at_ms,
        disposition,
    })
}

fn claim_runner_volume_instance_postgres(
    pool: &DbPool,
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
    runtime_claim: Option<&RunnerProcessRuntimeGrantClaim>,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    // Fleet authority always precedes child volume/key authority. Managed
    // execution effects hold the same singleton before fencing their lease
    // and revalidating this child, so taking the fleet lock first here avoids
    // a fleet -> lease -> volume / volume -> fleet deadlock cycle.
    tx.query_one(
        "SELECT singleton_id FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let row = tx
        .query_opt(
            "SELECT worker_id, current_epoch, status, active_instance_id, \
                    instance_lease_expires_at_ms \
               FROM jobs_runner_volumes WHERE volume_id = $1 FOR UPDATE",
            &[&request.volume_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let worker_id: String = row.get(0);
    let current_epoch: i64 = row.get(1);
    let status: String = row.get(2);
    let active_instance: Option<String> = row.get(3);
    let active_expiry: Option<i64> = row.get(4);
    validate_instance_claim_state(
        current_epoch,
        &status,
        active_instance.as_deref(),
        active_expiry,
        request.enrollment_epoch,
        request.process_instance_id,
        request.now_ms,
    )?;
    let trusted_runtime = match runtime_claim {
        Some(claim) => Some(bind_runner_process_runtime_postgres_tx(
            &mut tx,
            claim,
            &worker_id,
            request.volume_id,
            request.enrollment_epoch,
            request.process_instance_id,
            request.now_ms,
        )?),
        None => None,
    };
    if let Some(authority) = authority {
        consume_runner_volume_authority_postgres_tx(
            &mut tx,
            authority,
            "instance_claim",
            request.volume_id,
            request.enrollment_epoch,
            request.process_instance_id,
            request.now_ms,
        )?;
    }
    let disposition = if active_instance.as_deref() == Some(request.process_instance_id) {
        RunnerVolumeWriteDisposition::Replay
    } else {
        RunnerVolumeWriteDisposition::Applied
    };
    if disposition == RunnerVolumeWriteDisposition::Applied {
        invalidate_postgres_runner_fleet_cutover(&mut tx, request.now_ms)?;
    }
    if tx.execute(
        "UPDATE jobs_runner_volumes \
            SET active_instance_id = $2, instance_lease_expires_at_ms = $3, \
                status = CASE WHEN status = 'active' THEN 'reconciling' ELSE status END, \
                last_seen_at_ms = $4, updated_at_ms = $4 \
          WHERE volume_id = $1 AND current_epoch = $5",
        &[
            &request.volume_id,
            &request.process_instance_id,
            &request.lease_expires_at_ms,
            &request.now_ms,
            &request.enrollment_epoch,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeInstanceLease {
        volume_id: request.volume_id.to_string(),
        enrollment_epoch: request.enrollment_epoch,
        process_instance_id: request.process_instance_id.to_string(),
        runtime_grant_id: trusted_runtime
            .as_ref()
            .map(|runtime| runtime.runtime_grant_id.clone()),
        runtime_sha256: trusted_runtime.map(|runtime| runtime.runtime_sha256),
        lease_expires_at_ms: request.lease_expires_at_ms,
        disposition,
    })
}

fn validate_instance_claim_state(
    current_epoch: i64,
    status: &str,
    active_instance: Option<&str>,
    active_expiry: Option<i64>,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    if current_epoch != enrollment_epoch || status == "destroyed" {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    if active_instance.is_some_and(|instance| instance != process_instance_id)
        && active_expiry.is_some_and(|expiry| expiry > now_ms)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub fn heartbeat_runner_volume_instance(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    let request = RunnerVolumeInstanceLeaseRequest {
        volume_id,
        enrollment_epoch,
        process_instance_id,
        now_ms,
        lease_expires_at_ms,
    };
    heartbeat_runner_volume_instance_inner(pool, &request, false, None)
}

pub fn heartbeat_runner_volume_instance_authorized(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
    lease_expires_at_ms: i64,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    let request = RunnerVolumeInstanceLeaseRequest {
        volume_id,
        enrollment_epoch,
        process_instance_id,
        now_ms,
        lease_expires_at_ms,
    };
    heartbeat_runner_volume_instance_inner(pool, &request, true, Some(authority))
}

fn heartbeat_runner_volume_instance_inner(
    pool: &DbPool,
    request: &RunnerVolumeInstanceLeaseRequest<'_>,
    require_runtime: bool,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerVolumeInstanceLease> {
    validate_instance_lease_request(request)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let worker_id = tx
                .query_row(
                    "SELECT worker_id FROM jobs_runner_volumes \
                      WHERE volume_id = ?1 AND current_epoch = ?2 \
                        AND active_instance_id = ?3 AND instance_lease_expires_at_ms > ?4 \
                        AND status <> 'destroyed'",
                    params![
                        request.volume_id,
                        request.enrollment_epoch,
                        request.process_instance_id,
                        request.now_ms
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(RunnerVolumePurgeError::Conflict)?;
            let trusted_runtime = if require_runtime {
                Some(require_runner_process_runtime_sqlite_tx(
                    &tx,
                    &worker_id,
                    request.volume_id,
                    request.enrollment_epoch,
                    request.process_instance_id,
                )?)
            } else {
                None
            };
            if let Some(authority) = authority {
                consume_runner_volume_authority_sqlite_tx(
                    &tx,
                    authority,
                    "instance_heartbeat",
                    request.volume_id,
                    request.enrollment_epoch,
                    request.process_instance_id,
                    request.now_ms,
                )?;
            }
            let matched = tx.execute(
                "UPDATE jobs_runner_volumes \
                    SET instance_lease_expires_at_ms = ?4, last_seen_at_ms = ?5, \
                        updated_at_ms = ?5 \
                  WHERE volume_id = ?1 AND current_epoch = ?2 \
                    AND active_instance_id = ?3 AND instance_lease_expires_at_ms > ?5 \
                    AND status <> 'destroyed'",
                params![
                    request.volume_id,
                    request.enrollment_epoch,
                    request.process_instance_id,
                    request.lease_expires_at_ms,
                    request.now_ms,
                ],
            )?;
            if matched != 1 {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            tx.commit()?;
            Ok(RunnerVolumeInstanceLease {
                volume_id: request.volume_id.to_string(),
                enrollment_epoch: request.enrollment_epoch,
                process_instance_id: request.process_instance_id.to_string(),
                runtime_grant_id: trusted_runtime
                    .as_ref()
                    .map(|runtime| runtime.runtime_grant_id.clone()),
                runtime_sha256: trusted_runtime.map(|runtime| runtime.runtime_sha256),
                lease_expires_at_ms: request.lease_expires_at_ms,
                disposition: RunnerVolumeWriteDisposition::Applied,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let worker_id = tx
                .query_opt(
                    "SELECT worker_id FROM jobs_runner_volumes \
                      WHERE volume_id = $1 AND current_epoch = $2 \
                        AND active_instance_id = $3 AND instance_lease_expires_at_ms > $4 \
                        AND status <> 'destroyed' FOR UPDATE",
                    &[
                        &request.volume_id,
                        &request.enrollment_epoch,
                        &request.process_instance_id,
                        &request.now_ms,
                    ],
                )?
                .map(|row| row.get::<_, String>(0))
                .ok_or(RunnerVolumePurgeError::Conflict)?;
            let trusted_runtime = if require_runtime {
                Some(require_runner_process_runtime_postgres_tx(
                    &mut tx,
                    &worker_id,
                    request.volume_id,
                    request.enrollment_epoch,
                    request.process_instance_id,
                )?)
            } else {
                None
            };
            if let Some(authority) = authority {
                consume_runner_volume_authority_postgres_tx(
                    &mut tx,
                    authority,
                    "instance_heartbeat",
                    request.volume_id,
                    request.enrollment_epoch,
                    request.process_instance_id,
                    request.now_ms,
                )?;
            }
            let matched = tx.execute(
                "UPDATE jobs_runner_volumes \
                    SET instance_lease_expires_at_ms = $4, last_seen_at_ms = $5, \
                        updated_at_ms = $5 \
                  WHERE volume_id = $1 AND current_epoch = $2 \
                    AND active_instance_id = $3 AND instance_lease_expires_at_ms > $5 \
                    AND status <> 'destroyed'",
                &[
                    &request.volume_id,
                    &request.enrollment_epoch,
                    &request.process_instance_id,
                    &request.lease_expires_at_ms,
                    &request.now_ms,
                ],
            )?;
            if matched != 1 {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            tx.commit()?;
            Ok(RunnerVolumeInstanceLease {
                volume_id: request.volume_id.to_string(),
                enrollment_epoch: request.enrollment_epoch,
                process_instance_id: request.process_instance_id.to_string(),
                runtime_grant_id: trusted_runtime
                    .as_ref()
                    .map(|runtime| runtime.runtime_grant_id.clone()),
                runtime_sha256: trusted_runtime.map(|runtime| runtime.runtime_sha256),
                lease_expires_at_ms: request.lease_expires_at_ms,
                disposition: RunnerVolumeWriteDisposition::Applied,
            })
        }
    })
}

pub fn activate_reconciled_runner_volume(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    require_base64url(volume_id, 32)?;
    require_base64url(process_instance_id, 32)?;
    if enrollment_epoch <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.query_row(
                "SELECT singleton_id FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
                [],
                |_| Ok(()),
            )?;
            let row = tx
                .query_row(
                    "SELECT status, active_instance_id, instance_lease_expires_at_ms, \
                            required_tombstone_generation, \
                            reconciled_tombstone_generation, current_epoch, \
                            EXISTS (SELECT 1 \
                              FROM jobs_runner_volume_storage_attestations a \
                              JOIN jobs_runner_volume_keys k \
                                ON k.volume_id = jobs_runner_volumes.volume_id \
                               AND k.enrollment_epoch = jobs_runner_volumes.current_epoch \
                             WHERE a.volume_id = jobs_runner_volumes.volume_id \
                               AND a.enrollment_epoch = jobs_runner_volumes.current_epoch \
                               AND a.volume_key_fingerprint = k.key_fingerprint \
                               AND a.process_instance_id = \
                                   jobs_runner_volumes.active_instance_id \
                               AND a.enrollment_generation = \
                                   jobs_runner_volumes.enrollment_generation \
                               AND a.required_tombstone_generation = \
                                   jobs_runner_volumes.required_tombstone_generation \
                               AND a.reconciled_tombstone_generation = \
                                   jobs_runner_volumes.reconciled_tombstone_generation \
                               AND NOT EXISTS (SELECT 1 \
                                 FROM jobs_runner_volume_storage_attestations newer \
                                WHERE newer.volume_id = a.volume_id \
                                  AND newer.enrollment_epoch = a.enrollment_epoch \
                                  AND newer.attestation_generation > \
                                      a.attestation_generation)) \
                       FROM jobs_runner_volumes WHERE volume_id = ?1",
                    params![volume_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, bool>(6)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            validate_volume_activation(&row, enrollment_epoch, process_instance_id, now_ms)?;
            let disposition = if row.0 == "active" {
                RunnerVolumeWriteDisposition::Replay
            } else {
                RunnerVolumeWriteDisposition::Applied
            };
            tx.execute(
                "UPDATE jobs_runner_volumes SET status = 'active', updated_at_ms = ?2 \
                  WHERE volume_id = ?1",
                params![volume_id, now_ms],
            )?;
            tx.commit()?;
            Ok(disposition)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT singleton_id FROM jobs_runner_volume_fleet_state \
                  WHERE singleton_id = 1 FOR UPDATE",
                &[],
            )?;
            let row = tx
                .query_opt(
                    "SELECT status, active_instance_id, instance_lease_expires_at_ms, \
                            required_tombstone_generation, \
                            reconciled_tombstone_generation, current_epoch, \
                            EXISTS (SELECT 1 \
                              FROM jobs_runner_volume_storage_attestations a \
                              JOIN jobs_runner_volume_keys k \
                                ON k.volume_id = jobs_runner_volumes.volume_id \
                               AND k.enrollment_epoch = jobs_runner_volumes.current_epoch \
                             WHERE a.volume_id = jobs_runner_volumes.volume_id \
                               AND a.enrollment_epoch = jobs_runner_volumes.current_epoch \
                               AND a.volume_key_fingerprint = k.key_fingerprint \
                               AND a.process_instance_id = \
                                   jobs_runner_volumes.active_instance_id \
                               AND a.enrollment_generation = \
                                   jobs_runner_volumes.enrollment_generation \
                               AND a.required_tombstone_generation = \
                                   jobs_runner_volumes.required_tombstone_generation \
                               AND a.reconciled_tombstone_generation = \
                                   jobs_runner_volumes.reconciled_tombstone_generation \
                               AND NOT EXISTS (SELECT 1 \
                                 FROM jobs_runner_volume_storage_attestations newer \
                                WHERE newer.volume_id = a.volume_id \
                                  AND newer.enrollment_epoch = a.enrollment_epoch \
                                  AND newer.attestation_generation > \
                                      a.attestation_generation)) \
                       FROM jobs_runner_volumes WHERE volume_id = $1 FOR UPDATE",
                    &[&volume_id],
                )?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            let values = (
                row.get::<_, String>(0),
                row.get::<_, Option<String>>(1),
                row.get::<_, Option<i64>>(2),
                row.get::<_, i64>(3),
                row.get::<_, i64>(4),
                row.get::<_, i64>(5),
                row.get::<_, bool>(6),
            );
            validate_volume_activation(&values, enrollment_epoch, process_instance_id, now_ms)?;
            let disposition = if values.0 == "active" {
                RunnerVolumeWriteDisposition::Replay
            } else {
                RunnerVolumeWriteDisposition::Applied
            };
            tx.execute(
                "UPDATE jobs_runner_volumes SET status = 'active', updated_at_ms = $2 \
                  WHERE volume_id = $1",
                &[&volume_id, &now_ms],
            )?;
            tx.commit()?;
            Ok(disposition)
        }
    })
}

type RunnerVolumeActivationRow = (String, Option<String>, Option<i64>, i64, i64, i64, bool);

fn validate_volume_activation(
    row: &RunnerVolumeActivationRow,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    if !matches!(row.0.as_str(), "reconciling" | "active")
        || row.1.as_deref() != Some(process_instance_id)
        || row.2.is_none_or(|expiry| expiry <= now_ms)
        || row.3 != row.4
        || row.5 != enrollment_epoch
        || !row.6
    {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

pub fn require_active_reconciled_runner_volume(
    pool: &DbPool,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerVolumeRecord> {
    require_base64url(volume_id, 32)?;
    require_base64url(process_instance_id, 32)?;
    if enrollment_epoch <= 0 || now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let fleet_ready: bool = conn.query_row(
                "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
                  WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            let select = format!(
                "SELECT {RUNNER_VOLUME_COLUMNS} \
                   FROM jobs_runner_volumes v \
                   JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                  WHERE v.volume_id = ?1"
            );
            let volume = conn
                .query_row(&select, params![volume_id], runner_volume_from_sqlite_row)
                .optional()?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            let storage_attestation_current: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 \
                   FROM jobs_runner_volume_storage_attestations a \
                  WHERE a.volume_id = ?1 AND a.enrollment_epoch = ?2 \
                    AND a.volume_key_fingerprint = ?3 AND a.process_instance_id = ?4 \
                    AND a.enrollment_generation = ?5 \
                    AND a.required_tombstone_generation = ?6 \
                    AND a.reconciled_tombstone_generation = ?7 \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation))",
                params![
                    volume.volume_id,
                    volume.current_epoch,
                    volume.key_fingerprint,
                    process_instance_id,
                    volume.enrollment_generation,
                    volume.required_tombstone_generation,
                    volume.reconciled_tombstone_generation,
                ],
                |row| row.get(0),
            )?;
            validate_active_volume(
                &volume,
                enrollment_epoch,
                process_instance_id,
                now_ms,
                fleet_ready,
                storage_attestation_current,
            )?;
            Ok(volume)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let fleet_ready: bool = conn
                .query_one(
                    "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
                      WHERE singleton_id = 1",
                    &[],
                )?
                .get(0);
            let select = format!(
                "SELECT {RUNNER_VOLUME_COLUMNS} \
                   FROM jobs_runner_volumes v \
                   JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                  WHERE v.volume_id = $1"
            );
            let volume = conn
                .query_opt(&select, &[&volume_id])?
                .map(runner_volume_from_pg_row)
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            let storage_attestation_current: bool = conn
                .query_one(
                    "SELECT EXISTS(SELECT 1 \
                       FROM jobs_runner_volume_storage_attestations a \
                      WHERE a.volume_id = $1 AND a.enrollment_epoch = $2 \
                        AND a.volume_key_fingerprint = $3 AND a.process_instance_id = $4 \
                        AND a.enrollment_generation = $5 \
                        AND a.required_tombstone_generation = $6 \
                        AND a.reconciled_tombstone_generation = $7 \
                        AND NOT EXISTS (SELECT 1 \
                          FROM jobs_runner_volume_storage_attestations newer \
                         WHERE newer.volume_id = a.volume_id \
                           AND newer.enrollment_epoch = a.enrollment_epoch \
                           AND newer.attestation_generation > a.attestation_generation))",
                    &[
                        &volume.volume_id,
                        &volume.current_epoch,
                        &volume.key_fingerprint,
                        &process_instance_id,
                        &volume.enrollment_generation,
                        &volume.required_tombstone_generation,
                        &volume.reconciled_tombstone_generation,
                    ],
                )?
                .get(0);
            validate_active_volume(
                &volume,
                enrollment_epoch,
                process_instance_id,
                now_ms,
                fleet_ready,
                storage_attestation_current,
            )?;
            Ok(volume)
        }
    })
}

fn validate_active_volume(
    volume: &RunnerVolumeRecord,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
    fleet_ready: bool,
    storage_attestation_current: bool,
) -> RunnerVolumePurgeResult<()> {
    if !fleet_ready
        || !storage_attestation_current
        || volume.status != "active"
        || volume.current_epoch != enrollment_epoch
        || volume.active_instance_id.as_deref() != Some(process_instance_id)
        || volume
            .instance_lease_expires_at_ms
            .is_none_or(|expiry| expiry <= now_ms)
        || volume.required_tombstone_generation != volume.reconciled_tombstone_generation
    {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerAccountPurgeSubject {
    pub account_id: String,
    pub purge_subject: String,
    pub legacy_unresolved: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

fn runner_account_subject_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RunnerAccountPurgeSubject> {
    Ok(RunnerAccountPurgeSubject {
        account_id: row.get(0)?,
        purge_subject: row.get(1)?,
        legacy_unresolved: row.get::<_, i64>(2)? != 0,
        created_at_ms: row.get(3)?,
        updated_at_ms: row.get(4)?,
        disposition: RunnerVolumeWriteDisposition::Replay,
    })
}

fn runner_account_subject_from_pg_row(row: postgres::Row) -> RunnerAccountPurgeSubject {
    RunnerAccountPurgeSubject {
        account_id: row.get(0),
        purge_subject: row.get(1),
        legacy_unresolved: row.get::<_, i16>(2) != 0,
        created_at_ms: row.get(3),
        updated_at_ms: row.get(4),
        disposition: RunnerVolumeWriteDisposition::Replay,
    }
}

pub fn ensure_runner_account_purge_subject(
    pool: &DbPool,
    account_id: &str,
    legacy_unresolved: bool,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerAccountPurgeSubject> {
    let mut material = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut material);
    ensure_runner_account_purge_subject_with_material(
        pool,
        account_id,
        legacy_unresolved,
        now_ms,
        material,
        false,
    )
}

fn ensure_runner_account_purge_subject_with_material(
    pool: &DbPool,
    account_id: &str,
    legacy_unresolved: bool,
    now_ms: i64,
    subject_material: [u8; 32],
    allow_fenced: bool,
) -> RunnerVolumePurgeResult<RunnerAccountPurgeSubject> {
    require_nonempty_text(account_id, 240)?;
    if now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    let proposed_subject = encode_base64url(&subject_material);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            ensure_account_exists_sqlite(&tx, account_id)?;
            if !allow_fenced && sqlite_account_has_deletion_fence(&tx, account_id)? {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            let mut stored = tx
                .query_row(
                    "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, \
                            updated_at_ms FROM jobs_runner_account_subjects \
                      WHERE account_id = ?1",
                    params![account_id],
                    runner_account_subject_from_sqlite_row,
                )
                .optional()?;
            if let Some(subject) = stored.take() {
                if subject.legacy_unresolved != legacy_unresolved {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(subject);
            }
            tx.execute(
                "INSERT INTO jobs_runner_account_subjects ( \
                    account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![
                    account_id,
                    proposed_subject,
                    i64::from(legacy_unresolved),
                    now_ms
                ],
            )?;
            let mut created = tx.query_row(
                "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, \
                        updated_at_ms FROM jobs_runner_account_subjects WHERE account_id = ?1",
                params![account_id],
                runner_account_subject_from_sqlite_row,
            )?;
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_account_postgres(&mut tx, account_id)?;
            if !allow_fenced && postgres_account_has_deletion_fence(&mut tx, account_id)? {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            if let Some(row) = tx.query_opt(
                "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, \
                        updated_at_ms FROM jobs_runner_account_subjects \
                  WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )? {
                let stored = runner_account_subject_from_pg_row(row);
                if stored.legacy_unresolved != legacy_unresolved {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(stored);
            }
            let legacy_unresolved = i16::from(legacy_unresolved);
            tx.execute(
                "INSERT INTO jobs_runner_account_subjects ( \
                    account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
                 ) VALUES ($1, $2, $3, $4, $4)",
                &[&account_id, &proposed_subject, &legacy_unresolved, &now_ms],
            )?;
            let mut created = runner_account_subject_from_pg_row(tx.query_one(
                "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, \
                        updated_at_ms FROM jobs_runner_account_subjects WHERE account_id = $1",
                &[&account_id],
            )?);
            created.disposition = RunnerVolumeWriteDisposition::Applied;
            tx.commit()?;
            Ok(created)
        }
    })
}

pub fn lookup_runner_account_purge_subject(
    pool: &DbPool,
    account_id: &str,
) -> RunnerVolumePurgeResult<Option<RunnerAccountPurgeSubject>> {
    require_nonempty_text(account_id, 240)?;
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => Ok(pool
            .get()?
            .query_row(
                "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
               FROM jobs_runner_account_subjects WHERE account_id = ?1",
                params![account_id],
                runner_account_subject_from_sqlite_row,
            )
            .optional()?),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_opt(
                "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
               FROM jobs_runner_account_subjects WHERE account_id = $1",
                &[&account_id],
            )?
            .map(runner_account_subject_from_pg_row)),
    }
    })
}

fn ensure_account_exists_sqlite(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<()> {
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)",
        params![account_id],
        |row| row.get::<_, i64>(0),
    )? == 0
    {
        return Err(RunnerVolumePurgeError::NotFound);
    }
    Ok(())
}

fn sqlite_account_has_deletion_fence(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM account_deletion_intents WHERE account_id = ?1)",
        params![account_id],
        |row| row.get::<_, i64>(0),
    )? != 0)
}

fn lock_account_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<()> {
    if tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_none()
    {
        return Err(RunnerVolumePurgeError::NotFound);
    }
    Ok(())
}

fn postgres_account_has_deletion_fence(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<bool> {
    Ok(tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM account_deletion_intents WHERE account_id = $1)",
            &[&account_id],
        )?
        .get(0))
}

fn invalidate_sqlite_runner_fleet_cutover(
    tx: &RunnerSqliteTransaction<'_>,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "UPDATE jobs_runner_volume_fleet_state SET \
            cutover_state = CASE WHEN cutover_state = 'pre_cutover' \
                                 THEN 'pre_cutover' ELSE 'reconciling' END, \
            cutover_enrollment_generation = NULL, cutover_purge_generation = NULL, \
            cutover_tombstone_generation = NULL, cutover_destruction_generation = NULL, \
            cutover_legacy_reconciliation_generation = NULL, \
            cutover_storage_attestation_generation = NULL, \
            cutover_storage_attestation_count = NULL, \
            cutover_storage_attestation_set_sha256 = NULL, \
            cutover_legacy_inventory_generation = NULL, \
            cutover_legacy_inventory_reconciliation_id = NULL, \
            cutover_legacy_inventory_authority_id = NULL, \
            cutover_legacy_inventory_authority_sha256 = NULL, \
            cutover_legacy_inventory_root_count = NULL, \
            cutover_legacy_inventory_root_set_sha256 = NULL, \
            cutover_non_destroyed_volume_count = NULL, cutover_destruction_count = NULL, \
            cutover_unresolved_legacy_volume_count = NULL, cutover_evidence_ref = NULL, \
            cutover_evidence_sha256 = NULL, cutover_authorized_by = NULL, \
            cutover_at_ms = NULL, updated_at_ms = ?1 \
          WHERE singleton_id = 1",
        params![now_ms],
    )?;
    Ok(())
}

fn invalidate_postgres_runner_fleet_cutover(
    tx: &mut postgres::Transaction<'_>,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "UPDATE jobs_runner_volume_fleet_state SET \
            cutover_state = CASE WHEN cutover_state = 'pre_cutover' \
                                 THEN 'pre_cutover' ELSE 'reconciling' END, \
            cutover_enrollment_generation = NULL, cutover_purge_generation = NULL, \
            cutover_tombstone_generation = NULL, cutover_destruction_generation = NULL, \
            cutover_legacy_reconciliation_generation = NULL, \
            cutover_storage_attestation_generation = NULL, \
            cutover_storage_attestation_count = NULL, \
            cutover_storage_attestation_set_sha256 = NULL, \
            cutover_legacy_inventory_generation = NULL, \
            cutover_legacy_inventory_reconciliation_id = NULL, \
            cutover_legacy_inventory_authority_id = NULL, \
            cutover_legacy_inventory_authority_sha256 = NULL, \
            cutover_legacy_inventory_root_count = NULL, \
            cutover_legacy_inventory_root_set_sha256 = NULL, \
            cutover_non_destroyed_volume_count = NULL, cutover_destruction_count = NULL, \
            cutover_unresolved_legacy_volume_count = NULL, cutover_evidence_ref = NULL, \
            cutover_evidence_sha256 = NULL, cutover_authorized_by = NULL, \
            cutover_at_ms = NULL, updated_at_ms = $1 \
          WHERE singleton_id = 1",
        &[&now_ms],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeRequestStatus {
    pub request_id: String,
    pub account_id: Option<String>,
    pub purge_subject: String,
    pub purge_generation: i64,
    pub legacy_inventory_generation: i64,
    pub legacy_inventory_reconciliation_id: String,
    pub legacy_inventory_authority_id: String,
    pub legacy_inventory_authority_sha256: String,
    pub state: String,
    pub legacy_unresolved_count: i64,
    pub required_target_count: i64,
    pub resolved_target_count: i64,
    pub target_set_sha256: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedRunnerVolumePurge {
    pub status: RunnerPurgeRequestStatus,
    pub commands: Vec<RunnerPurgeCommand>,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareRunnerVolumePurgeRequest {
    pub request_id: String,
    pub account_id: String,
    pub minimum_runner_build_id: String,
    pub expected_legacy_inventory_generation: i64,
    pub expected_legacy_inventory_reconciliation_id: String,
    pub expected_legacy_inventory_authority_id: String,
    pub expected_legacy_inventory_authority_sha256: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerPurgeTargetIdentity {
    volume_id: String,
    enrollment_epoch: i64,
    key_fingerprint: String,
}

fn purge_request_status_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RunnerPurgeRequestStatus> {
    Ok(RunnerPurgeRequestStatus {
        request_id: row.get(0)?,
        account_id: row.get(1)?,
        purge_subject: row.get(2)?,
        purge_generation: row.get(3)?,
        legacy_inventory_generation: row.get(4)?,
        legacy_inventory_reconciliation_id: row.get(5)?,
        legacy_inventory_authority_id: row.get(6)?,
        legacy_inventory_authority_sha256: row.get(7)?,
        state: row.get(8)?,
        legacy_unresolved_count: row.get(9)?,
        required_target_count: row.get(10)?,
        resolved_target_count: row.get(11)?,
        target_set_sha256: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
        completed_at_ms: row.get(15)?,
    })
}

fn purge_request_status_from_pg_row(row: postgres::Row) -> RunnerPurgeRequestStatus {
    RunnerPurgeRequestStatus {
        request_id: row.get(0),
        account_id: row.get(1),
        purge_subject: row.get(2),
        purge_generation: row.get(3),
        legacy_inventory_generation: row.get(4),
        legacy_inventory_reconciliation_id: row.get(5),
        legacy_inventory_authority_id: row.get(6),
        legacy_inventory_authority_sha256: row.get(7),
        state: row.get(8),
        legacy_unresolved_count: row.get(9),
        required_target_count: row.get(10),
        resolved_target_count: row.get(11),
        target_set_sha256: row.get(12),
        created_at_ms: row.get(13),
        updated_at_ms: row.get(14),
        completed_at_ms: row.get(15),
    }
}

const PURGE_REQUEST_STATUS_COLUMNS: &str =
    "request_id, account_id, purge_subject, purge_generation, \
     legacy_inventory_generation, legacy_inventory_reconciliation_id, \
     legacy_inventory_authority_id, legacy_inventory_authority_sha256, state, \
     legacy_unresolved_count, required_target_count, resolved_target_count, \
     target_set_sha256, created_at_ms, updated_at_ms, completed_at_ms";

fn validate_prepare_runner_volume_purge(
    input: &PrepareRunnerVolumePurgeRequest,
) -> RunnerVolumePurgeResult<()> {
    require_runner_identifier(&input.request_id)?;
    require_nonempty_text(&input.account_id, 240)?;
    if !runner_build_id_is_canonical(&input.minimum_runner_build_id) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    require_runner_identifier(&input.expected_legacy_inventory_reconciliation_id)?;
    require_runner_identifier(&input.expected_legacy_inventory_authority_id)?;
    require_sha256(&input.expected_legacy_inventory_authority_sha256)?;
    if input.expected_legacy_inventory_generation <= 0 || input.now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

fn runner_target_set_sha256(
    targets: &[RunnerPurgeTargetIdentity],
) -> RunnerVolumePurgeResult<String> {
    let mut previous: Option<(&str, i64)> = None;
    let mut digest = Sha256::new();
    digest.update(RUNNER_TARGET_SET_DOMAIN.as_bytes());
    digest.update(b"\n");
    for target in targets {
        require_base64url(&target.volume_id, 32)?;
        require_sha256(&target.key_fingerprint)?;
        if target.enrollment_epoch <= 0
            || previous.is_some_and(|previous| {
                previous >= (target.volume_id.as_str(), target.enrollment_epoch)
            })
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        digest.update(b"volume_id=");
        digest.update(target.volume_id.as_bytes());
        digest.update(b"\nenrollment_epoch=");
        digest.update(target.enrollment_epoch.to_string().as_bytes());
        digest.update(b"\nkey_fingerprint=");
        digest.update(target.key_fingerprint.as_bytes());
        digest.update(b"\n");
        previous = Some((&target.volume_id, target.enrollment_epoch));
    }
    Ok(hex::encode(digest.finalize()))
}

fn runner_purge_successor_request_id(
    deletion_request_id: &str,
    legacy_inventory_generation: i64,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-runner\0purge-successor-request-v1\0");
    digest.update(deletion_request_id.as_bytes());
    digest.update(b"\0");
    digest.update(legacy_inventory_generation.to_string().as_bytes());
    format!("purge-successor-{}", hex::encode(digest.finalize()))
}

fn merge_runner_purge_targets(
    targets: impl IntoIterator<Item = RunnerPurgeTargetIdentity>,
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeTargetIdentity>> {
    let mut merged = BTreeMap::<(String, i64), RunnerPurgeTargetIdentity>::new();
    for target in targets {
        let key = (target.volume_id.clone(), target.enrollment_epoch);
        match merged.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(target);
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                if entry.get().key_fingerprint != target.key_fingerprint {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
            }
        }
    }
    Ok(merged.into_values().collect())
}

fn runner_purge_command_id(
    request_id: &str,
    volume_id: &str,
    enrollment_epoch: i64,
    enforcement: bool,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-runner\0purge-command-id-v1\0");
    let command_kind: &[u8] = if enforcement {
        b"enforcement\0"
    } else {
        b"target\0"
    };
    digest.update(command_kind);
    digest.update(request_id.as_bytes());
    digest.update(b"\0");
    digest.update(volume_id.as_bytes());
    digest.update(b"\0");
    digest.update(enrollment_epoch.to_string().as_bytes());
    format!("purge-{}", hex::encode(digest.finalize()))
}

fn parse_stored_runner_purge_command(
    command_json: &str,
    command_sha256: &str,
    server_key_id: &str,
    server_signature: &str,
    key_ring: &RunnerPurgeCommandKeyRing,
) -> RunnerVolumePurgeResult<RunnerPurgeCommand> {
    let command: RunnerPurgeCommand =
        serde_json::from_str(command_json).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    if command.command_sha256()? != command_sha256
        || command.server_key_id != server_key_id
        || command.signature != server_signature
        || serde_json::to_string(&command).map_err(anyhow::Error::from)? != command_json
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    key_ring.verify_command(&command)?;
    Ok(command)
}

fn load_sqlite_purge_commands(
    tx: &RunnerSqliteTransaction<'_>,
    request_id: &str,
    key_ring: &RunnerPurgeCommandKeyRing,
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeCommand>> {
    let mut statement = tx.prepare(
        "SELECT command_json, command_sha256, server_key_id, server_signature \
           FROM jobs_runner_purge_targets WHERE request_id = ?1 \
          ORDER BY volume_id, volume_epoch",
    )?;
    let rows = statement.query_map(params![request_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut commands = Vec::new();
    for row in rows {
        let (json, sha256, key_id, signature) = row?;
        commands.push(parse_stored_runner_purge_command(
            &json, &sha256, &key_id, &signature, key_ring,
        )?);
    }
    Ok(commands)
}

fn load_postgres_purge_commands(
    tx: &mut postgres::Transaction<'_>,
    request_id: &str,
    key_ring: &RunnerPurgeCommandKeyRing,
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeCommand>> {
    tx.query(
        "SELECT command_json, command_sha256, server_key_id, server_signature \
           FROM jobs_runner_purge_targets WHERE request_id = $1 \
          ORDER BY volume_id, volume_epoch",
        &[&request_id],
    )?
    .into_iter()
    .map(|row| {
        parse_stored_runner_purge_command(
            row.get::<_, String>(0).as_str(),
            row.get::<_, String>(1).as_str(),
            row.get::<_, String>(2).as_str(),
            row.get::<_, String>(3).as_str(),
            key_ring,
        )
    })
    .collect()
}

pub fn prepare_runner_volume_purge(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
) -> RunnerVolumePurgeResult<PreparedRunnerVolumePurge> {
    let mut subject_material = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut subject_material);
    prepare_runner_volume_purge_with_subject_material(
        pool,
        signer,
        key_ring,
        input,
        subject_material,
    )
}

fn prepare_runner_volume_purge_with_subject_material(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
    subject_material: [u8; 32],
) -> RunnerVolumePurgeResult<PreparedRunnerVolumePurge> {
    validate_prepare_runner_volume_purge(input)?;
    key_ring.require_current_signer(signer)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => prepare_runner_volume_purge_sqlite(
            pool,
            signer,
            key_ring,
            input,
            encode_base64url(&subject_material),
        ),
        DbPool::Postgres(_) => prepare_runner_volume_purge_postgres(
            pool,
            signer,
            key_ring,
            input,
            encode_base64url(&subject_material),
        ),
    })
}

fn prepare_runner_volume_purge_sqlite(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
    proposed_subject: String,
) -> RunnerVolumePurgeResult<PreparedRunnerVolumePurge> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let fleet = tx.query_row(
        "SELECT purge_generation, legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        },
    )?;
    if fleet.1 != "ready" {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    if fleet.2 != input.expected_legacy_inventory_generation
        || fleet.3.as_deref() != Some(input.expected_legacy_inventory_reconciliation_id.as_str())
        || fleet.4.as_deref() != Some(input.expected_legacy_inventory_authority_id.as_str())
        || fleet.5.as_deref() != Some(input.expected_legacy_inventory_authority_sha256.as_str())
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    ensure_account_exists_sqlite(&tx, &input.account_id)?;
    if !sqlite_account_has_deletion_fence(&tx, &input.account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let status_select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests WHERE request_id = ?1"
    );
    let current_attempt_select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests \
          WHERE account_id = ?1 AND deletion_request_id = ?2 \
            AND legacy_inventory_generation = ?3 \
            AND legacy_inventory_reconciliation_id = ?4 \
            AND legacy_inventory_authority_id = ?5 \
            AND legacy_inventory_authority_sha256 = ?6 \
          ORDER BY purge_generation DESC LIMIT 1"
    );
    if let Some(status) = tx
        .query_row(
            &current_attempt_select,
            params![
                input.account_id,
                input.request_id,
                input.expected_legacy_inventory_generation,
                input.expected_legacy_inventory_reconciliation_id,
                input.expected_legacy_inventory_authority_id,
                input.expected_legacy_inventory_authority_sha256,
            ],
            purge_request_status_from_sqlite_row,
        )
        .optional()?
    {
        let commands = load_sqlite_purge_commands(&tx, &status.request_id, key_ring)?;
        tx.commit()?;
        return Ok(PreparedRunnerVolumePurge {
            status,
            commands,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    if let Some((deletion_request_id, account_id)) = tx
        .query_row(
            "SELECT deletion_request_id, account_id FROM jobs_runner_purge_requests \
              WHERE request_id = ?1",
            params![input.request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        if deletion_request_id != input.request_id
            || account_id.as_deref() != Some(input.account_id.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
    }
    let predecessor_request_id = tx
        .query_row(
            "SELECT request_id FROM jobs_runner_purge_requests \
              WHERE account_id = ?1 AND deletion_request_id = ?2 \
              ORDER BY purge_generation DESC LIMIT 1",
            params![input.account_id, input.request_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if predecessor_request_id.is_none()
        && tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests WHERE account_id = ?1)",
            params![input.account_id],
            |row| row.get::<_, i64>(0),
        )? != 0
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let request_id = predecessor_request_id.as_ref().map_or_else(
        || input.request_id.clone(),
        |_| {
            runner_purge_successor_request_id(
                &input.request_id,
                input.expected_legacy_inventory_generation,
            )
        },
    );
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests WHERE request_id = ?1)",
        params![request_id],
        |row| row.get::<_, i64>(0),
    )? != 0
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let subject =
        ensure_sqlite_subject_for_prepare(&tx, &input.account_id, &proposed_subject, input.now_ms)?;
    let targets = load_sqlite_frozen_targets(&tx, &input.account_id, &input.request_id)?;
    let target_set_sha256 = runner_target_set_sha256(&targets)?;
    let purge_generation = fleet
        .0
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let unbound_legacy_count = sqlite_legacy_account_storage_count(&tx, &input.account_id)?;
    let observed_legacy_unresolved_count = i64::from(subject.legacy_unresolved)
        .checked_add(unbound_legacy_count)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let previous_legacy_unresolved_count = tx.query_row(
        "SELECT COALESCE(MAX(legacy_unresolved_count), 0) \
           FROM jobs_runner_purge_requests \
          WHERE account_id = ?1 AND deletion_request_id = ?2",
        params![input.account_id, input.request_id],
        |row| row.get::<_, i64>(0),
    )?;
    let legacy_unresolved_count =
        observed_legacy_unresolved_count.max(previous_legacy_unresolved_count);
    let required_target_count =
        i64::try_from(targets.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_requests ( \
            request_id, deletion_request_id, predecessor_request_id, account_id, \
            purge_subject, purge_generation, \
            legacy_inventory_generation, legacy_inventory_reconciliation_id, \
            legacy_inventory_authority_id, legacy_inventory_authority_sha256, state, \
            legacy_unresolved_count, required_target_count, resolved_target_count, \
            target_set_sha256, created_at_ms, updated_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', \
                   ?11, ?12, 0, ?13, ?14, ?14)",
        params![
            request_id,
            input.request_id,
            predecessor_request_id,
            input.account_id,
            subject.purge_subject,
            purge_generation,
            input.expected_legacy_inventory_generation,
            input.expected_legacy_inventory_reconciliation_id,
            input.expected_legacy_inventory_authority_id,
            input.expected_legacy_inventory_authority_sha256,
            legacy_unresolved_count,
            required_target_count,
            target_set_sha256,
            input.now_ms,
        ],
    )?;
    let mut attempt_input = input.clone();
    attempt_input.request_id = request_id.clone();
    let commands = insert_sqlite_frozen_targets(
        &tx,
        signer,
        key_ring,
        &attempt_input,
        &subject.purge_subject,
        purge_generation,
        &targets,
    )?;
    let initially_resolved_count = tx.query_row(
        "SELECT COUNT(*) FROM jobs_runner_purge_targets \
          WHERE request_id = ?1 AND state = 'destroyed'",
        params![request_id],
        |row| row.get::<_, i64>(0),
    )?;
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET resolved_target_count = ?2 \
          WHERE request_id = ?1",
        params![request_id, initially_resolved_count],
    )?;
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET state = 'superseded', superseded_at_ms = ?3, updated_at_ms = ?3 \
          WHERE account_id = ?1 AND deletion_request_id = ?2 \
            AND request_id <> ?4 AND state = 'pending'",
        params![input.account_id, input.request_id, input.now_ms, request_id],
    )?;
    invalidate_sqlite_runner_fleet_cutover(&tx, input.now_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET purge_generation = ?1, updated_at_ms = ?2 \
          WHERE singleton_id = 1 AND purge_generation = ?3",
        params![purge_generation, input.now_ms, fleet.0],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let status = tx.query_row(
        &status_select,
        params![request_id],
        purge_request_status_from_sqlite_row,
    )?;
    tx.commit()?;
    Ok(PreparedRunnerVolumePurge {
        status,
        commands,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn ensure_sqlite_subject_for_prepare(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
    proposed_subject: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerAccountPurgeSubject> {
    if let Some(subject) = tx
        .query_row(
            "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
               FROM jobs_runner_account_subjects WHERE account_id = ?1",
            params![account_id],
            runner_account_subject_from_sqlite_row,
        )
        .optional()?
    {
        return Ok(subject);
    }
    tx.execute(
        "INSERT INTO jobs_runner_account_subjects ( \
            account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
         ) VALUES (?1, ?2, 0, ?3, ?3)",
        params![account_id, proposed_subject, now_ms],
    )?;
    Ok(tx.query_row(
        "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
           FROM jobs_runner_account_subjects WHERE account_id = ?1",
        params![account_id],
        runner_account_subject_from_sqlite_row,
    )?)
}

fn load_sqlite_frozen_targets(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
    deletion_request_id: &str,
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeTargetIdentity>> {
    let mut statement = tx.prepare(
        "SELECT candidate.volume_id, candidate.volume_epoch, candidate.key_fingerprint \
           FROM ( \
             SELECT v.volume_id, v.current_epoch AS volume_epoch, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.status <> 'destroyed' \
             UNION ALL \
             SELECT t.volume_id, t.volume_epoch, t.volume_key_fingerprint \
               FROM jobs_runner_purge_targets t \
               JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
              WHERE r.account_id = ?1 AND r.deletion_request_id = ?2 \
           ) candidate \
          GROUP BY candidate.volume_id, candidate.volume_epoch, candidate.key_fingerprint \
          ORDER BY candidate.volume_id, candidate.volume_epoch",
    )?;
    let rows = statement.query_map(params![account_id, deletion_request_id], |row| {
        Ok(RunnerPurgeTargetIdentity {
            volume_id: row.get(0)?,
            enrollment_epoch: row.get(1)?,
            key_fingerprint: row.get(2)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn sqlite_legacy_account_storage_count(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<i64> {
    Ok(tx.query_row(
        "SELECT \
          (SELECT COUNT(*) FROM jobs_execution_leases e \
            LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = e.run_id \
           WHERE e.account_id = ?1 AND b.run_id IS NULL) \
          + (SELECT COUNT(*) FROM jobs_browser_sessions s \
              LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = s.id \
             WHERE s.account_id = ?1 AND b.run_id IS NULL) \
          + (SELECT COUNT(*) FROM jobs_local_run_tickets t \
              LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = t.id \
             WHERE t.account_id = ?1 AND b.run_id IS NULL) \
          + (SELECT COUNT(*) FROM jobs_browser_profile_snapshots s \
              LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = s.writer_run_id \
             WHERE s.account_id = ?1 AND b.run_id IS NULL) \
          + (SELECT COUNT(*) FROM ( \
              SELECT DISTINCT e.run_id FROM jobs_run_events e \
               LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = e.run_id \
              WHERE e.account_id = ?1 AND b.run_id IS NULL \
            ) unbound_run_events)",
        params![account_id],
        |row| row.get(0),
    )?)
}

fn insert_sqlite_frozen_targets(
    tx: &RunnerSqliteTransaction<'_>,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
    purge_subject: &str,
    purge_generation: i64,
    targets: &[RunnerPurgeTargetIdentity],
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeCommand>> {
    let mut commands = Vec::with_capacity(targets.len());
    for target in targets {
        let command = signer.sign_command(NewRunnerPurgeCommand {
            request_id: input.request_id.clone(),
            command_id: runner_purge_command_id(
                &input.request_id,
                &target.volume_id,
                target.enrollment_epoch,
                false,
            ),
            target_volume_id: target.volume_id.clone(),
            target_key_fingerprint: target.key_fingerprint.clone(),
            enrollment_epoch: target.enrollment_epoch,
            purge_subject: purge_subject.to_string(),
            purge_generation,
            storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            legacy_inventory_authority_generation: input.expected_legacy_inventory_generation,
            legacy_inventory_authority_sha256: input
                .expected_legacy_inventory_authority_sha256
                .clone(),
            issued_at_ms: input.now_ms,
            minimum_runner_build_id: input.minimum_runner_build_id.clone(),
        })?;
        key_ring.verify_command(&command)?;
        let command_json = serde_json::to_string(&command).map_err(anyhow::Error::from)?;
        let command_sha256 = command.command_sha256()?;
        let destruction_id = tx
            .query_row(
                "SELECT destruction_id FROM jobs_runner_volume_destructions \
                  WHERE volume_id = ?1 AND volume_epoch = ?2 \
                    AND volume_key_fingerprint = ?3",
                params![
                    target.volume_id,
                    target.enrollment_epoch,
                    target.key_fingerprint,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let state = if destruction_id.is_some() {
            "destroyed"
        } else {
            "pending"
        };
        tx.execute(
            "INSERT INTO jobs_runner_purge_targets ( \
                request_id, volume_id, volume_epoch, volume_key_fingerprint, command_id, \
                command_json, command_sha256, server_key_id, server_signature, \
                state, destruction_id, created_at_ms, updated_at_ms \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                input.request_id,
                target.volume_id,
                target.enrollment_epoch,
                target.key_fingerprint,
                command.command_id,
                command_json,
                command_sha256,
                command.server_key_id,
                command.signature,
                state,
                destruction_id,
                input.now_ms,
            ],
        )?;
        if state == "pending" {
            commands.push(command);
        }
    }
    Ok(commands)
}

fn prepare_runner_volume_purge_postgres(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
    proposed_subject: String,
) -> RunnerVolumePurgeResult<PreparedRunnerVolumePurge> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    // Global lock order for fan-out: fleet singleton, account, then the
    // complete sorted volume set. No connection escapes this transaction.
    let fleet = tx.query_one(
        "SELECT purge_generation, legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let fleet_purge_generation: i64 = fleet.get(0);
    let fleet_legacy_state: String = fleet.get(1);
    let fleet_legacy_generation: i64 = fleet.get(2);
    let fleet_legacy_reconciliation_id: Option<String> = fleet.get(3);
    let fleet_legacy_authority_id: Option<String> = fleet.get(4);
    let fleet_legacy_authority_sha256: Option<String> = fleet.get(5);
    if fleet_legacy_state != "ready" {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    if fleet_legacy_generation != input.expected_legacy_inventory_generation
        || fleet_legacy_reconciliation_id.as_deref()
            != Some(input.expected_legacy_inventory_reconciliation_id.as_str())
        || fleet_legacy_authority_id.as_deref()
            != Some(input.expected_legacy_inventory_authority_id.as_str())
        || fleet_legacy_authority_sha256.as_deref()
            != Some(input.expected_legacy_inventory_authority_sha256.as_str())
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    lock_account_postgres(&mut tx, &input.account_id)?;
    if !postgres_account_has_deletion_fence(&mut tx, &input.account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let status_select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests WHERE request_id = $1 FOR UPDATE"
    );
    let current_attempt_select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests \
          WHERE account_id = $1 AND deletion_request_id = $2 \
            AND legacy_inventory_generation = $3 \
            AND legacy_inventory_reconciliation_id = $4 \
            AND legacy_inventory_authority_id = $5 \
            AND legacy_inventory_authority_sha256 = $6 \
          ORDER BY purge_generation DESC LIMIT 1 FOR UPDATE"
    );
    if let Some(row) = tx.query_opt(
        &current_attempt_select,
        &[
            &input.account_id,
            &input.request_id,
            &input.expected_legacy_inventory_generation,
            &input.expected_legacy_inventory_reconciliation_id,
            &input.expected_legacy_inventory_authority_id,
            &input.expected_legacy_inventory_authority_sha256,
        ],
    )? {
        let status = purge_request_status_from_pg_row(row);
        let commands = load_postgres_purge_commands(&mut tx, &status.request_id, key_ring)?;
        tx.commit()?;
        return Ok(PreparedRunnerVolumePurge {
            status,
            commands,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    if let Some(row) = tx.query_opt(
        "SELECT deletion_request_id, account_id FROM jobs_runner_purge_requests \
          WHERE request_id = $1 FOR UPDATE",
        &[&input.request_id],
    )? {
        let deletion_request_id: String = row.get(0);
        let account_id: Option<String> = row.get(1);
        if deletion_request_id != input.request_id
            || account_id.as_deref() != Some(input.account_id.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
    }
    let predecessor_request_id = tx
        .query_opt(
            "SELECT request_id FROM jobs_runner_purge_requests \
              WHERE account_id = $1 AND deletion_request_id = $2 \
              ORDER BY purge_generation DESC LIMIT 1 FOR UPDATE",
            &[&input.account_id, &input.request_id],
        )?
        .map(|row| row.get::<_, String>(0));
    if predecessor_request_id.is_none()
        && tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests WHERE account_id = $1)",
                &[&input.account_id],
            )?
            .get::<_, bool>(0)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let request_id = predecessor_request_id.as_ref().map_or_else(
        || input.request_id.clone(),
        |_| {
            runner_purge_successor_request_id(
                &input.request_id,
                input.expected_legacy_inventory_generation,
            )
        },
    );
    if tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests WHERE request_id = $1)",
            &[&request_id],
        )?
        .get::<_, bool>(0)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let subject = ensure_postgres_subject_for_prepare(
        &mut tx,
        &input.account_id,
        &proposed_subject,
        input.now_ms,
    )?;
    let targets = load_postgres_frozen_targets(&mut tx, &input.account_id, &input.request_id)?;
    let target_set_sha256 = runner_target_set_sha256(&targets)?;
    let purge_generation = fleet_purge_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let unbound_legacy_count = postgres_legacy_account_storage_count(&mut tx, &input.account_id)?;
    let observed_legacy_unresolved_count = i64::from(subject.legacy_unresolved)
        .checked_add(unbound_legacy_count)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    let previous_legacy_unresolved_count: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(legacy_unresolved_count), 0) \
               FROM jobs_runner_purge_requests \
              WHERE account_id = $1 AND deletion_request_id = $2",
            &[&input.account_id, &input.request_id],
        )?
        .get(0);
    let legacy_unresolved_count =
        observed_legacy_unresolved_count.max(previous_legacy_unresolved_count);
    let required_target_count =
        i64::try_from(targets.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_requests ( \
            request_id, deletion_request_id, predecessor_request_id, account_id, \
            purge_subject, purge_generation, \
            legacy_inventory_generation, legacy_inventory_reconciliation_id, \
            legacy_inventory_authority_id, legacy_inventory_authority_sha256, state, \
            legacy_unresolved_count, required_target_count, resolved_target_count, \
            target_set_sha256, created_at_ms, updated_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', \
                   $11, $12, 0, $13, $14, $14)",
        &[
            &request_id,
            &input.request_id,
            &predecessor_request_id,
            &input.account_id,
            &subject.purge_subject,
            &purge_generation,
            &input.expected_legacy_inventory_generation,
            &input.expected_legacy_inventory_reconciliation_id,
            &input.expected_legacy_inventory_authority_id,
            &input.expected_legacy_inventory_authority_sha256,
            &legacy_unresolved_count,
            &required_target_count,
            &target_set_sha256,
            &input.now_ms,
        ],
    )?;
    let mut attempt_input = input.clone();
    attempt_input.request_id = request_id.clone();
    let commands = insert_postgres_frozen_targets(
        &mut tx,
        signer,
        key_ring,
        &attempt_input,
        &subject.purge_subject,
        purge_generation,
        &targets,
    )?;
    let initially_resolved_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_runner_purge_targets \
              WHERE request_id = $1 AND state = 'destroyed'",
            &[&request_id],
        )?
        .get(0);
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET resolved_target_count = $2 \
          WHERE request_id = $1",
        &[&request_id, &initially_resolved_count],
    )?;
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET state = 'superseded', superseded_at_ms = $3, updated_at_ms = $3 \
          WHERE account_id = $1 AND deletion_request_id = $2 \
            AND request_id <> $4 AND state = 'pending'",
        &[
            &input.account_id,
            &input.request_id,
            &input.now_ms,
            &request_id,
        ],
    )?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, input.now_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET purge_generation = $1, updated_at_ms = $2 \
          WHERE singleton_id = 1 AND purge_generation = $3",
        &[&purge_generation, &input.now_ms, &fleet_purge_generation],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let status = purge_request_status_from_pg_row(tx.query_one(&status_select, &[&request_id])?);
    tx.commit()?;
    Ok(PreparedRunnerVolumePurge {
        status,
        commands,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn ensure_postgres_subject_for_prepare(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    proposed_subject: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerAccountPurgeSubject> {
    if let Some(row) = tx.query_opt(
        "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
           FROM jobs_runner_account_subjects WHERE account_id = $1 FOR UPDATE",
        &[&account_id],
    )? {
        return Ok(runner_account_subject_from_pg_row(row));
    }
    // The account row is already locked, so absence is stable for this
    // transaction. Preparation is intentionally mutation-free: the caller may
    // still need to wait on later effect rows and must sample its authoritative
    // clock before creating the subject.
    Ok(RunnerAccountPurgeSubject {
        account_id: account_id.to_string(),
        purge_subject: proposed_subject.to_string(),
        legacy_unresolved: false,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn load_postgres_frozen_targets(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    deletion_request_id: &str,
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeTargetIdentity>> {
    let current = tx
        .query(
            "SELECT v.volume_id, v.current_epoch, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.status <> 'destroyed' \
              ORDER BY v.volume_id, v.current_epoch \
              FOR UPDATE OF v, k",
            &[],
        )?
        .into_iter()
        .map(|row| RunnerPurgeTargetIdentity {
            volume_id: row.get(0),
            enrollment_epoch: row.get(1),
            key_fingerprint: row.get(2),
        })
        .collect::<Vec<_>>();
    let historical = tx
        .query(
            "SELECT t.volume_id, t.volume_epoch, t.volume_key_fingerprint \
               FROM jobs_runner_purge_targets t \
               JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
              WHERE r.account_id = $1 AND r.deletion_request_id = $2 \
              ORDER BY t.volume_id, t.volume_epoch FOR UPDATE OF t, r",
            &[&account_id, &deletion_request_id],
        )?
        .into_iter()
        .map(|row| RunnerPurgeTargetIdentity {
            volume_id: row.get(0),
            enrollment_epoch: row.get(1),
            key_fingerprint: row.get(2),
        });
    merge_runner_purge_targets(current.into_iter().chain(historical))
}

fn postgres_legacy_account_storage_count(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> RunnerVolumePurgeResult<i64> {
    Ok(tx
        .query_one(
            "SELECT \
              (SELECT COUNT(*) FROM jobs_execution_leases e \
                LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = e.run_id \
               WHERE e.account_id = $1 AND b.run_id IS NULL) \
              + (SELECT COUNT(*) FROM jobs_browser_sessions s \
                  LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = s.id \
                 WHERE s.account_id = $1 AND b.run_id IS NULL) \
              + (SELECT COUNT(*) FROM jobs_local_run_tickets t \
                  LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = t.id \
                 WHERE t.account_id = $1 AND b.run_id IS NULL) \
              + (SELECT COUNT(*) FROM jobs_browser_profile_snapshots s \
                  LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = s.writer_run_id \
                 WHERE s.account_id = $1 AND b.run_id IS NULL) \
              + (SELECT COUNT(*) FROM ( \
                  SELECT DISTINCT e.run_id FROM jobs_run_events e \
                   LEFT JOIN jobs_execution_lease_volume_bindings b ON b.run_id = e.run_id \
                  WHERE e.account_id = $1 AND b.run_id IS NULL \
                ) unbound_run_events)",
            &[&account_id],
        )?
        .get(0))
}

fn insert_postgres_frozen_targets(
    tx: &mut postgres::Transaction<'_>,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PrepareRunnerVolumePurgeRequest,
    purge_subject: &str,
    purge_generation: i64,
    targets: &[RunnerPurgeTargetIdentity],
) -> RunnerVolumePurgeResult<Vec<RunnerPurgeCommand>> {
    let mut commands = Vec::with_capacity(targets.len());
    for target in targets {
        let command = signer.sign_command(NewRunnerPurgeCommand {
            request_id: input.request_id.clone(),
            command_id: runner_purge_command_id(
                &input.request_id,
                &target.volume_id,
                target.enrollment_epoch,
                false,
            ),
            target_volume_id: target.volume_id.clone(),
            target_key_fingerprint: target.key_fingerprint.clone(),
            enrollment_epoch: target.enrollment_epoch,
            purge_subject: purge_subject.to_string(),
            purge_generation,
            storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            legacy_inventory_authority_generation: input.expected_legacy_inventory_generation,
            legacy_inventory_authority_sha256: input
                .expected_legacy_inventory_authority_sha256
                .clone(),
            issued_at_ms: input.now_ms,
            minimum_runner_build_id: input.minimum_runner_build_id.clone(),
        })?;
        key_ring.verify_command(&command)?;
        let command_json = serde_json::to_string(&command).map_err(anyhow::Error::from)?;
        let command_sha256 = command.command_sha256()?;
        let destruction_id = tx
            .query_opt(
                "SELECT destruction_id FROM jobs_runner_volume_destructions \
                  WHERE volume_id = $1 AND volume_epoch = $2 \
                    AND volume_key_fingerprint = $3 FOR UPDATE",
                &[
                    &target.volume_id,
                    &target.enrollment_epoch,
                    &target.key_fingerprint,
                ],
            )?
            .map(|row| row.get::<_, String>(0));
        let state = if destruction_id.is_some() {
            "destroyed"
        } else {
            "pending"
        };
        tx.execute(
            "INSERT INTO jobs_runner_purge_targets ( \
                request_id, volume_id, volume_epoch, volume_key_fingerprint, command_id, \
                command_json, command_sha256, server_key_id, server_signature, \
                state, destruction_id, created_at_ms, updated_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)",
            &[
                &input.request_id,
                &target.volume_id,
                &target.enrollment_epoch,
                &target.key_fingerprint,
                &command.command_id,
                &command_json,
                &command_sha256,
                &command.server_key_id,
                &command.signature,
                &state,
                &destruction_id,
                &input.now_ms,
            ],
        )?;
        if state == "pending" {
            commands.push(command);
        }
    }
    Ok(commands)
}

pub fn runner_volume_purge_status(
    pool: &DbPool,
    request_id: &str,
) -> RunnerVolumePurgeResult<RunnerPurgeRequestStatus> {
    require_runner_identifier(request_id)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let select = format!(
                "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
                   FROM jobs_runner_purge_requests WHERE request_id = ?1"
            );
            conn.query_row(
                &select,
                params![request_id],
                purge_request_status_from_sqlite_row,
            )
            .optional()?
            .ok_or(RunnerVolumePurgeError::NotFound)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let select = format!(
                "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
                   FROM jobs_runner_purge_requests WHERE request_id = $1"
            );
            conn.query_opt(&select, &[&request_id])?
                .map(purge_request_status_from_pg_row)
                .ok_or(RunnerVolumePurgeError::NotFound)
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PollRunnerVolumePurgeCommandsRequest {
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub minimum_runner_build_id: String,
    pub now_ms: i64,
    pub limit: usize,
    pub after_command_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeCommandPage {
    pub commands: Vec<RunnerPurgeCommand>,
    pub next_after_command_id: Option<String>,
}

fn validate_poll_runner_volume_purge_commands(
    input: &PollRunnerVolumePurgeCommandsRequest,
) -> RunnerVolumePurgeResult<()> {
    require_base64url(&input.volume_id, 32)?;
    require_base64url(&input.process_instance_id, 32)?;
    if !runner_build_id_is_canonical(&input.minimum_runner_build_id) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    if input.enrollment_epoch <= 0
        || input.now_ms < 0
        || input.limit == 0
        || input.limit > MAX_RUNNER_PURGE_POLL_COMMANDS
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    if let Some(after_command_id) = input.after_command_id.as_deref() {
        require_runner_identifier(after_command_id)?;
    }
    Ok(())
}

#[cfg(debug_assertions)]
pub fn poll_runner_volume_purge_commands(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    poll_runner_volume_purge_commands_inner(pool, signer, key_ring, input, None)
}

pub fn poll_runner_volume_purge_commands_authorized(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    poll_runner_volume_purge_commands_inner(pool, signer, key_ring, input, Some(authority))
}

fn poll_runner_volume_purge_commands_inner(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    validate_poll_runner_volume_purge_commands(input)?;
    key_ring.require_current_signer(signer)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            poll_runner_volume_purge_commands_sqlite(pool, signer, key_ring, input, authority)
        }
        DbPool::Postgres(_) => {
            poll_runner_volume_purge_commands_postgres(pool, signer, key_ring, input, authority)
        }
    })
}

fn poll_runner_volume_purge_commands_sqlite(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.query_row(
        "SELECT singleton_id FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |_| Ok(()),
    )?;
    let (required_generation, reconciled_generation, key_fingerprint) =
        validate_poll_volume_sqlite(&tx, input)?;
    if let Some(authority) = authority {
        consume_runner_volume_authority_sqlite_tx(
            &tx,
            authority,
            "purge_poll",
            &input.volume_id,
            input.enrollment_epoch,
            &input.process_instance_id,
            input.now_ms,
        )?;
    }
    let missing = load_sqlite_missing_enforcements(
        &tx,
        &input.volume_id,
        input.enrollment_epoch,
        required_generation,
        reconciled_generation,
    )?;
    for missing in missing {
        insert_sqlite_enforcement_command(
            &tx,
            signer,
            key_ring,
            input,
            &missing.request_id,
            &missing.purge_subject,
            missing.purge_generation,
            missing.legacy_inventory_authority_generation,
            &missing.legacy_inventory_authority_sha256,
            &key_fingerprint,
        )?;
    }
    let commands = load_sqlite_pending_volume_commands(&tx, key_ring, input)?;
    tx.commit()?;
    Ok(commands)
}

fn validate_poll_volume_sqlite(
    tx: &RunnerSqliteTransaction<'_>,
    input: &PollRunnerVolumePurgeCommandsRequest,
) -> RunnerVolumePurgeResult<(i64, i64, String)> {
    let row = tx
        .query_row(
            "SELECT v.current_epoch, v.status, v.active_instance_id, \
                    v.instance_lease_expires_at_ms, v.required_tombstone_generation, \
                    v.reconciled_tombstone_generation, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = ?1",
            params![input.volume_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    validate_poll_volume_values(row.0, &row.1, row.2.as_deref(), row.3, input)?;
    Ok((row.4, row.5, row.6))
}

fn validate_poll_volume_values(
    current_epoch: i64,
    status: &str,
    active_instance_id: Option<&str>,
    instance_lease_expires_at_ms: Option<i64>,
    input: &PollRunnerVolumePurgeCommandsRequest,
) -> RunnerVolumePurgeResult<()> {
    if current_epoch != input.enrollment_epoch
        || status == "destroyed"
        || active_instance_id != Some(input.process_instance_id.as_str())
        || instance_lease_expires_at_ms.is_none_or(|expiry| expiry <= input.now_ms)
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok(())
}

#[derive(Debug)]
struct MissingRunnerPurgeEnforcement {
    request_id: String,
    purge_subject: String,
    purge_generation: i64,
    legacy_inventory_authority_generation: i64,
    legacy_inventory_authority_sha256: String,
}

fn load_sqlite_missing_enforcements(
    tx: &RunnerSqliteTransaction<'_>,
    volume_id: &str,
    enrollment_epoch: i64,
    required_generation: i64,
    reconciled_generation: i64,
) -> RunnerVolumePurgeResult<Vec<MissingRunnerPurgeEnforcement>> {
    let mut statement = tx.prepare(
        "SELECT r.request_id, r.purge_subject, r.purge_generation, \
                r.legacy_inventory_generation, r.legacy_inventory_authority_sha256 \
           FROM jobs_runner_purge_tombstones s \
           JOIN jobs_runner_purge_requests r ON r.request_id = s.request_id \
          WHERE s.tombstone_generation <= ?3 AND s.tombstone_generation > ?4 \
            AND NOT EXISTS ( \
                SELECT 1 FROM jobs_runner_purge_targets t \
                 WHERE t.request_id = r.request_id AND t.volume_id = ?1 \
                   AND t.volume_epoch = ?2 \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM jobs_runner_purge_enforcements e \
                 WHERE e.request_id = r.request_id AND e.volume_id = ?1 \
                   AND e.volume_epoch = ?2 \
            ) \
          ORDER BY s.tombstone_generation, r.request_id",
    )?;
    let rows = statement.query_map(
        params![
            volume_id,
            enrollment_epoch,
            required_generation,
            reconciled_generation,
        ],
        |row| {
            Ok(MissingRunnerPurgeEnforcement {
                request_id: row.get(0)?,
                purge_subject: row.get(1)?,
                purge_generation: row.get(2)?,
                legacy_inventory_authority_generation: row.get(3)?,
                legacy_inventory_authority_sha256: row.get(4)?,
            })
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn insert_sqlite_enforcement_command(
    tx: &RunnerSqliteTransaction<'_>,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    request_id: &str,
    purge_subject: &str,
    purge_generation: i64,
    legacy_inventory_authority_generation: i64,
    legacy_inventory_authority_sha256: &str,
    key_fingerprint: &str,
) -> RunnerVolumePurgeResult<()> {
    let command = signer.sign_command(NewRunnerPurgeCommand {
        request_id: request_id.to_string(),
        command_id: runner_purge_command_id(
            request_id,
            &input.volume_id,
            input.enrollment_epoch,
            true,
        ),
        target_volume_id: input.volume_id.clone(),
        target_key_fingerprint: key_fingerprint.to_string(),
        enrollment_epoch: input.enrollment_epoch,
        purge_subject: purge_subject.to_string(),
        purge_generation,
        storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
        subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
        legacy_inventory_authority_generation,
        legacy_inventory_authority_sha256: legacy_inventory_authority_sha256.to_string(),
        issued_at_ms: input.now_ms,
        minimum_runner_build_id: input.minimum_runner_build_id.clone(),
    })?;
    key_ring.verify_command(&command)?;
    let command_json = serde_json::to_string(&command).map_err(anyhow::Error::from)?;
    let command_sha256 = command.command_sha256()?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_enforcements ( \
            request_id, volume_id, volume_epoch, volume_key_fingerprint, command_id, \
            command_json, command_sha256, server_key_id, server_signature, state, \
            created_at_ms, updated_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?10)",
        params![
            request_id,
            input.volume_id,
            input.enrollment_epoch,
            key_fingerprint,
            command.command_id,
            command_json,
            command_sha256,
            command.server_key_id,
            command.signature,
            input.now_ms,
        ],
    )?;
    Ok(())
}

fn load_sqlite_pending_volume_commands(
    tx: &RunnerSqliteTransaction<'_>,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    let cursor = input
        .after_command_id
        .as_deref()
        .map(|command_id| {
            tx.query_row(
                "SELECT order_class, order_generation, command_id FROM ( \
                   SELECT 1 AS order_class, r.purge_generation AS order_generation, \
                          t.command_id \
                     FROM jobs_runner_purge_targets t \
                    JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
                    WHERE t.volume_id = ?1 AND t.volume_epoch = ?2 \
                   UNION ALL \
                   SELECT 0 AS order_class, s.tombstone_generation AS order_generation, \
                          e.command_id \
                     FROM jobs_runner_purge_enforcements e \
                     JOIN jobs_runner_purge_requests r ON r.request_id = e.request_id \
                    JOIN jobs_runner_purge_tombstones s ON s.request_id = e.request_id \
                    WHERE e.volume_id = ?1 AND e.volume_epoch = ?2 \
                 ) pending WHERE command_id = ?3",
                params![input.volume_id, input.enrollment_epoch, command_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(RunnerVolumePurgeError::InvalidRequest)
        })
        .transpose()?;
    let (cursor_class, cursor_generation, cursor_command_id) =
        cursor.unwrap_or((-1, -1, String::new()));
    let query_limit = i64::try_from(input.limit.saturating_add(1))
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    let mut statement = tx.prepare(
        "SELECT command_id, command_json, command_sha256, server_key_id, server_signature \
           FROM ( \
             SELECT 1 AS order_class, r.purge_generation AS order_generation, \
                    t.command_id, t.command_json, t.command_sha256, \
                    t.server_key_id, t.server_signature \
               FROM jobs_runner_purge_targets t \
               JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
              WHERE t.volume_id = ?1 AND t.volume_epoch = ?2 AND t.state = 'pending' \
                AND r.state = 'pending' \
             UNION ALL \
             SELECT 0 AS order_class, s.tombstone_generation AS order_generation, \
                    e.command_id, e.command_json, e.command_sha256, \
                    e.server_key_id, e.server_signature \
               FROM jobs_runner_purge_enforcements e \
               JOIN jobs_runner_purge_requests r ON r.request_id = e.request_id \
               JOIN jobs_runner_purge_tombstones s ON s.request_id = e.request_id \
              WHERE e.volume_id = ?1 AND e.volume_epoch = ?2 AND e.state = 'pending' \
           ) pending \
          WHERE ?3 IS NULL \
             OR order_class > ?4 \
             OR (order_class = ?4 AND order_generation > ?5) \
             OR (order_class = ?4 AND order_generation = ?5 AND command_id > ?6) \
          ORDER BY order_class, order_generation, command_id LIMIT ?7",
    )?;
    let rows = statement.query_map(
        params![
            input.volume_id,
            input.enrollment_epoch,
            input.after_command_id,
            cursor_class,
            cursor_generation,
            cursor_command_id,
            query_limit,
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    let mut commands = Vec::new();
    for row in rows {
        let (command_id, json, sha256, key_id, signature) = row?;
        let command =
            parse_stored_runner_purge_command(&json, &sha256, &key_id, &signature, key_ring)?;
        if command.command_id != command_id {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        commands.push(command);
    }
    let has_more = commands.len() > input.limit;
    commands.truncate(input.limit);
    let next_after_command_id = if has_more {
        commands.last().map(|command| command.command_id.clone())
    } else {
        None
    };
    Ok(RunnerPurgeCommandPage {
        commands,
        next_after_command_id,
    })
}

fn poll_runner_volume_purge_commands_postgres(
    pool: &DbPool,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    authority: Option<&VerifiedRunnerVolumeAuthority>,
) -> RunnerVolumePurgeResult<RunnerPurgeCommandPage> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.query_one(
        "SELECT singleton_id FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let row = tx
        .query_opt(
            "SELECT v.current_epoch, v.status, v.active_instance_id, \
                    v.instance_lease_expires_at_ms, v.required_tombstone_generation, \
                    v.reconciled_tombstone_generation, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = $1 FOR UPDATE OF v, k",
            &[&input.volume_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    validate_poll_volume_values(
        row.get(0),
        row.get::<_, String>(1).as_str(),
        row.get::<_, Option<String>>(2).as_deref(),
        row.get(3),
        input,
    )?;
    if let Some(authority) = authority {
        consume_runner_volume_authority_postgres_tx(
            &mut tx,
            authority,
            "purge_poll",
            &input.volume_id,
            input.enrollment_epoch,
            &input.process_instance_id,
            input.now_ms,
        )?;
    }
    let required_generation: i64 = row.get(4);
    let reconciled_generation: i64 = row.get(5);
    let key_fingerprint: String = row.get(6);
    let missing = tx.query(
        "SELECT r.request_id, r.purge_subject, r.purge_generation, \
                r.legacy_inventory_generation, r.legacy_inventory_authority_sha256 \
           FROM jobs_runner_purge_tombstones s \
           JOIN jobs_runner_purge_requests r ON r.request_id = s.request_id \
          WHERE s.tombstone_generation <= $3 AND s.tombstone_generation > $4 \
            AND NOT EXISTS ( \
                SELECT 1 FROM jobs_runner_purge_targets t \
                 WHERE t.request_id = r.request_id AND t.volume_id = $1 \
                   AND t.volume_epoch = $2 \
            ) \
            AND NOT EXISTS ( \
                SELECT 1 FROM jobs_runner_purge_enforcements e \
                 WHERE e.request_id = r.request_id AND e.volume_id = $1 \
                   AND e.volume_epoch = $2 \
            ) \
          ORDER BY s.tombstone_generation, r.request_id FOR UPDATE OF r, s",
        &[
            &input.volume_id,
            &input.enrollment_epoch,
            &required_generation,
            &reconciled_generation,
        ],
    )?;
    for row in missing {
        insert_postgres_enforcement_command(
            &mut tx,
            signer,
            key_ring,
            input,
            row.get::<_, String>(0).as_str(),
            row.get::<_, String>(1).as_str(),
            row.get(2),
            row.get(3),
            row.get::<_, String>(4).as_str(),
            &key_fingerprint,
        )?;
    }
    let cursor = if let Some(command_id) = input.after_command_id.as_deref() {
        tx.query_opt(
            "SELECT order_class, order_generation, command_id FROM ( \
               SELECT 1::BIGINT AS order_class, r.purge_generation AS order_generation, \
                      t.command_id \
                 FROM jobs_runner_purge_targets t \
                JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
                WHERE t.volume_id = $1 AND t.volume_epoch = $2 \
               UNION ALL \
               SELECT 0::BIGINT AS order_class, s.tombstone_generation AS order_generation, \
                      e.command_id \
                 FROM jobs_runner_purge_enforcements e \
                 JOIN jobs_runner_purge_requests r ON r.request_id = e.request_id \
                JOIN jobs_runner_purge_tombstones s ON s.request_id = e.request_id \
                WHERE e.volume_id = $1 AND e.volume_epoch = $2 \
             ) pending WHERE command_id = $3",
            &[&input.volume_id, &input.enrollment_epoch, &command_id],
        )?
        .map(|row| {
            (
                row.get::<_, i64>(0),
                row.get::<_, i64>(1),
                row.get::<_, String>(2),
            )
        })
        .ok_or(RunnerVolumePurgeError::InvalidRequest)?
    } else {
        (-1, -1, String::new())
    };
    let query_limit = i64::try_from(input.limit.saturating_add(1))
        .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
    let rows = tx.query(
        "SELECT command_id, command_json, command_sha256, server_key_id, server_signature \
           FROM ( \
             SELECT 1 AS order_class, r.purge_generation AS order_generation, \
                    t.command_id, t.command_json, t.command_sha256, \
                    t.server_key_id, t.server_signature \
               FROM jobs_runner_purge_targets t \
               JOIN jobs_runner_purge_requests r ON r.request_id = t.request_id \
              WHERE t.volume_id = $1 AND t.volume_epoch = $2 AND t.state = 'pending' \
                AND r.state = 'pending' \
             UNION ALL \
             SELECT 0 AS order_class, s.tombstone_generation AS order_generation, \
                    e.command_id, e.command_json, e.command_sha256, \
                    e.server_key_id, e.server_signature \
               FROM jobs_runner_purge_enforcements e \
               JOIN jobs_runner_purge_requests r ON r.request_id = e.request_id \
               JOIN jobs_runner_purge_tombstones s ON s.request_id = e.request_id \
              WHERE e.volume_id = $1 AND e.volume_epoch = $2 AND e.state = 'pending' \
           ) pending \
          WHERE $3::TEXT IS NULL \
             OR order_class > $4 \
             OR (order_class = $4 AND order_generation > $5) \
             OR (order_class = $4 AND order_generation = $5 AND command_id > $6) \
          ORDER BY order_class, order_generation, command_id LIMIT $7",
        &[
            &input.volume_id,
            &input.enrollment_epoch,
            &input.after_command_id,
            &cursor.0,
            &cursor.1,
            &cursor.2,
            &query_limit,
        ],
    )?;
    let mut commands = rows
        .into_iter()
        .map(|row| {
            let command_id: String = row.get(0);
            let command = parse_stored_runner_purge_command(
                row.get::<_, String>(1).as_str(),
                row.get::<_, String>(2).as_str(),
                row.get::<_, String>(3).as_str(),
                row.get::<_, String>(4).as_str(),
                key_ring,
            )?;
            if command.command_id != command_id {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            Ok(command)
        })
        .collect::<RunnerVolumePurgeResult<Vec<_>>>()?;
    let has_more = commands.len() > input.limit;
    commands.truncate(input.limit);
    let next_after_command_id = if has_more {
        commands.last().map(|command| command.command_id.clone())
    } else {
        None
    };
    tx.commit()?;
    Ok(RunnerPurgeCommandPage {
        commands,
        next_after_command_id,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_postgres_enforcement_command(
    tx: &mut postgres::Transaction<'_>,
    signer: &RunnerPurgeSigner,
    key_ring: &RunnerPurgeCommandKeyRing,
    input: &PollRunnerVolumePurgeCommandsRequest,
    request_id: &str,
    purge_subject: &str,
    purge_generation: i64,
    legacy_inventory_authority_generation: i64,
    legacy_inventory_authority_sha256: &str,
    key_fingerprint: &str,
) -> RunnerVolumePurgeResult<()> {
    let command = signer.sign_command(NewRunnerPurgeCommand {
        request_id: request_id.to_string(),
        command_id: runner_purge_command_id(
            request_id,
            &input.volume_id,
            input.enrollment_epoch,
            true,
        ),
        target_volume_id: input.volume_id.clone(),
        target_key_fingerprint: key_fingerprint.to_string(),
        enrollment_epoch: input.enrollment_epoch,
        purge_subject: purge_subject.to_string(),
        purge_generation,
        storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
        subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
        legacy_inventory_authority_generation,
        legacy_inventory_authority_sha256: legacy_inventory_authority_sha256.to_string(),
        issued_at_ms: input.now_ms,
        minimum_runner_build_id: input.minimum_runner_build_id.clone(),
    })?;
    key_ring.verify_command(&command)?;
    let command_json = serde_json::to_string(&command).map_err(anyhow::Error::from)?;
    let command_sha256 = command.command_sha256()?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_enforcements ( \
            request_id, volume_id, volume_epoch, volume_key_fingerprint, command_id, \
            command_json, command_sha256, server_key_id, server_signature, state, \
            created_at_ms, updated_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $10)",
        &[
            &request_id,
            &input.volume_id,
            &input.enrollment_epoch,
            &key_fingerprint,
            &command.command_id,
            &command_json,
            &command_sha256,
            &command.server_key_id,
            &command.signature,
            &input.now_ms,
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeAckOutcome {
    pub disposition: RunnerVolumeWriteDisposition,
    pub status: RunnerPurgeRequestStatus,
    pub reconciled_tombstone_generation: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredPurgeCommandKind {
    Target,
    Enforcement,
}

#[derive(Debug, Clone)]
struct StoredPurgeCommandForAck {
    kind: StoredPurgeCommandKind,
    state: String,
    request_state: Option<String>,
    command: RunnerPurgeCommand,
    command_sha256: String,
    ack_json: Option<String>,
    ack_sha256: Option<String>,
    ack_signature: Option<String>,
}

pub fn acknowledge_runner_volume_purge(
    pool: &DbPool,
    key_ring: &RunnerPurgeCommandKeyRing,
    ack: &RunnerPurgeAck,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeAckOutcome> {
    ack.validate_unsigned()?;
    if received_at_ms < 0 || ack.completed_at_ms > received_at_ms.saturating_add(300_000) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            acknowledge_runner_volume_purge_sqlite(pool, key_ring, ack, received_at_ms)
        }
        DbPool::Postgres(_) => {
            acknowledge_runner_volume_purge_postgres(pool, key_ring, ack, received_at_ms)
        }
    })
}

fn validate_ack_against_command(
    ack: &RunnerPurgeAck,
    command: &RunnerPurgeCommand,
    command_sha256: &str,
) -> RunnerVolumePurgeResult<()> {
    if ack.request_id != command.request_id
        || ack.command_id != command.command_id
        || ack.command_sha256 != command_sha256
        || ack.target_volume_id != command.target_volume_id
        || ack.target_key_fingerprint != command.target_key_fingerprint
        || ack.enrollment_epoch != command.enrollment_epoch
        || ack.purge_subject_sha256 != runner_purge_subject_sha256(&command.purge_subject)?
        || ack.purge_generation != command.purge_generation
        || ack.storage_evidence.version != command.storage_evidence_version
        || ack.storage_evidence.subject_storage.layout_version
            != command.subject_storage_layout_version
        || ack.completed_at_ms < command.issued_at_ms
        || !runner_build_satisfies(&ack.runner_build_id, &command.minimum_runner_build_id)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn parse_runner_build_id(value: &str) -> Option<(u32, u32)> {
    fn parse_component(value: &str) -> Option<u32> {
        if value.is_empty()
            || value.len() > 9
            || !value.bytes().all(|byte| byte.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return None;
        }
        value.parse().ok()
    }

    let version = value.strip_prefix("runner-")?;
    let mut components = version.split('.');
    let generation = parse_component(components.next()?)?;
    let revision = match components.next() {
        Some(revision) => parse_component(revision)?,
        None => 0,
    };
    if components.next().is_some() {
        return None;
    }
    Some((generation, revision))
}

pub fn runner_build_id_is_canonical(value: &str) -> bool {
    parse_runner_build_id(value).is_some()
}

pub fn runner_build_satisfies(actual: &str, minimum: &str) -> bool {
    match (
        parse_runner_build_id(actual),
        parse_runner_build_id(minimum),
    ) {
        (Some(actual), Some(minimum)) => actual >= minimum,
        _ => false,
    }
}

fn acknowledge_runner_volume_purge_sqlite(
    pool: &DbPool,
    key_ring: &RunnerPurgeCommandKeyRing,
    ack: &RunnerPurgeAck,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeAckOutcome> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let volume = load_sqlite_ack_volume(&tx, ack)?;
    verify_runner_purge_ack(ack, &volume.4)?;
    let stored = load_sqlite_purge_command_for_ack(&tx, key_ring, ack)?;
    validate_ack_against_command(ack, &stored.command, &stored.command_sha256)?;
    let ack_json = serde_json::to_string(ack).map_err(anyhow::Error::from)?;
    let ack_sha256 = ack.ack_sha256()?;
    if stored.state == "acknowledged" {
        if stored.ack_json.as_deref() != Some(ack_json.as_str())
            || stored.ack_sha256.as_deref() != Some(ack_sha256.as_str())
            || stored.ack_signature.as_deref() != Some(ack.signature.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        let status = load_sqlite_purge_status_tx(&tx, &ack.request_id)?;
        tx.commit()?;
        return Ok(RunnerPurgeAckOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            status,
            reconciled_tombstone_generation: volume.3,
        });
    }
    if stored.state != "pending"
        || (stored.kind == StoredPurgeCommandKind::Target
            && stored.request_state.as_deref() != Some("pending"))
        || volume.0 != ack.enrollment_epoch
        || volume.1 == "destroyed"
        || volume.2.as_deref() != Some(ack.process_instance_id.as_str())
        || volume.5.is_none_or(|expiry| expiry <= received_at_ms)
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    update_sqlite_stored_ack(
        &tx,
        stored.kind,
        ack,
        &ack_json,
        &ack_sha256,
        received_at_ms,
    )?;
    record_sqlite_purged_residency(&tx, ack, received_at_ms)?;
    let status = recompute_sqlite_purge_request(&tx, &ack.request_id, received_at_ms)?;
    let reconciled = reconcile_sqlite_volume_cursor(
        &tx,
        &ack.target_volume_id,
        ack.enrollment_epoch,
        received_at_ms,
    )?;
    tx.commit()?;
    Ok(RunnerPurgeAckOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        status,
        reconciled_tombstone_generation: reconciled,
    })
}

/// (current epoch, status, active instance, reconciled generation, public key,
/// instance lease expiry)
type RunnerAckVolume = (i64, String, Option<String>, i64, String, Option<i64>);

fn load_sqlite_ack_volume(
    tx: &RunnerSqliteTransaction<'_>,
    ack: &RunnerPurgeAck,
) -> RunnerVolumePurgeResult<RunnerAckVolume> {
    let row = tx
        .query_row(
            "SELECT v.current_epoch, v.status, v.active_instance_id, \
                    v.reconciled_tombstone_generation, k.public_key_base64url, \
                    v.instance_lease_expires_at_ms, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = ?2 \
              WHERE v.volume_id = ?1",
            params![ack.target_volume_id, ack.enrollment_epoch],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    if row.6 != ack.target_key_fingerprint {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok((row.0, row.1, row.2, row.3, row.4, row.5))
}

fn load_sqlite_purge_command_for_ack(
    tx: &RunnerSqliteTransaction<'_>,
    key_ring: &RunnerPurgeCommandKeyRing,
    ack: &RunnerPurgeAck,
) -> RunnerVolumePurgeResult<StoredPurgeCommandForAck> {
    let query = |table: &str| -> RunnerVolumePurgeResult<Option<StoredPurgeCommandForAck>> {
        let source = if table == "jobs_runner_purge_targets" {
            "jobs_runner_purge_targets AS command \
             JOIN jobs_runner_purge_requests AS request \
               ON request.request_id = command.request_id"
        } else {
            "jobs_runner_purge_enforcements AS command"
        };
        let request_state_column = if table == "jobs_runner_purge_targets" {
            "request.state"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT command.state, command.command_json, command.command_sha256, \
                    command.server_key_id, command.server_signature, command.ack_json, \
                    command.ack_sha256, command.ack_signature, {request_state_column} \
               FROM {source} \
              WHERE command.request_id = ?1 AND command.volume_id = ?2 \
                AND command.volume_epoch = ?3 AND command.command_id = ?4"
        );
        let row = tx
            .query_row(
                &sql,
                params![
                    ack.request_id,
                    ack.target_volume_id,
                    ack.enrollment_epoch,
                    ack.command_id,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let command = parse_stored_runner_purge_command(&row.1, &row.2, &row.3, &row.4, key_ring)?;
        Ok(Some(StoredPurgeCommandForAck {
            kind: if table == "jobs_runner_purge_targets" {
                StoredPurgeCommandKind::Target
            } else {
                StoredPurgeCommandKind::Enforcement
            },
            state: row.0,
            request_state: row.8,
            command,
            command_sha256: row.2,
            ack_json: row.5,
            ack_sha256: row.6,
            ack_signature: row.7,
        }))
    };
    if let Some(target) = query("jobs_runner_purge_targets")? {
        return Ok(target);
    }
    query("jobs_runner_purge_enforcements")?.ok_or(RunnerVolumePurgeError::NotFound)
}

fn update_sqlite_stored_ack(
    tx: &RunnerSqliteTransaction<'_>,
    kind: StoredPurgeCommandKind,
    ack: &RunnerPurgeAck,
    ack_json: &str,
    ack_sha256: &str,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    let table = match kind {
        StoredPurgeCommandKind::Target => "jobs_runner_purge_targets",
        StoredPurgeCommandKind::Enforcement => "jobs_runner_purge_enforcements",
    };
    let sql = format!(
        "UPDATE {table} \
            SET state = 'acknowledged', ack_json = ?5, ack_sha256 = ?6, \
                ack_signature = ?7, ack_process_instance_id = ?8, \
                ack_command_sha256 = ?9, ack_inventory_before_count = ?10, \
                ack_inventory_before_sha256 = ?11, ack_inventory_after_count = ?12, \
                ack_inventory_after_sha256 = ?13, ack_removed_entry_count = ?14, \
                ack_runner_build_id = ?15, ack_completed_at_ms = ?16, \
                updated_at_ms = ?17, acknowledged_at_ms = ?17 \
          WHERE request_id = ?1 AND volume_id = ?2 AND volume_epoch = ?3 \
            AND command_id = ?4 AND state = 'pending'"
    );
    if tx.execute(
        &sql,
        params![
            ack.request_id,
            ack.target_volume_id,
            ack.enrollment_epoch,
            ack.command_id,
            ack_json,
            ack_sha256,
            ack.signature,
            ack.process_instance_id,
            ack.command_sha256,
            ack.before_inventory_count,
            ack.before_inventory_sha256,
            ack.after_inventory_count,
            ack.after_inventory_sha256,
            ack.removed_count,
            ack.runner_build_id,
            ack.completed_at_ms,
            received_at_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn record_sqlite_purged_residency(
    tx: &RunnerSqliteTransaction<'_>,
    ack: &RunnerPurgeAck,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    let purge_subject: String = tx
        .query_row(
            "SELECT purge_subject FROM jobs_runner_purge_requests WHERE request_id = ?1",
            params![ack.request_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    if runner_purge_subject_sha256(&purge_subject)? != ack.purge_subject_sha256 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let existing = tx
        .query_row(
            "SELECT state, purge_generation, first_recorded_at_ms \
               FROM jobs_runner_volume_residencies \
              WHERE purge_subject = ?1 AND volume_id = ?2 AND volume_epoch = ?3",
            params![purge_subject, ack.target_volume_id, ack.enrollment_epoch],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    if existing
        .as_ref()
        .is_some_and(|(_, generation, _)| *generation > ack.purge_generation)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let first_recorded_at_ms = existing.map_or(received_at_ms, |value| value.2);
    tx.execute(
        "INSERT INTO jobs_runner_volume_residencies ( \
            purge_subject, volume_id, volume_epoch, state, purge_generation, \
            first_recorded_at_ms, last_recorded_at_ms \
         ) VALUES (?1, ?2, ?3, 'purged', ?4, ?5, ?6) \
         ON CONFLICT(purge_subject, volume_id, volume_epoch) DO UPDATE SET \
            state = 'purged', purge_generation = excluded.purge_generation, \
            last_recorded_at_ms = excluded.last_recorded_at_ms",
        params![
            purge_subject,
            ack.target_volume_id,
            ack.enrollment_epoch,
            ack.purge_generation,
            first_recorded_at_ms,
            received_at_ms,
        ],
    )?;
    Ok(())
}

fn load_sqlite_purge_status_tx(
    tx: &RunnerSqliteTransaction<'_>,
    request_id: &str,
) -> RunnerVolumePurgeResult<RunnerPurgeRequestStatus> {
    let select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests WHERE request_id = ?1"
    );
    tx.query_row(
        &select,
        params![request_id],
        purge_request_status_from_sqlite_row,
    )
    .optional()?
    .ok_or(RunnerVolumePurgeError::NotFound)
}

fn recompute_sqlite_purge_request(
    tx: &RunnerSqliteTransaction<'_>,
    request_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeRequestStatus> {
    let current = load_sqlite_purge_status_tx(tx, request_id)?;
    let mut statement = tx.prepare(
        "SELECT volume_id, volume_epoch, volume_key_fingerprint, state \
           FROM jobs_runner_purge_targets WHERE request_id = ?1 \
          ORDER BY volume_id, volume_epoch",
    )?;
    let rows = statement.query_map(params![request_id], |row| {
        Ok((
            RunnerPurgeTargetIdentity {
                volume_id: row.get(0)?,
                enrollment_epoch: row.get(1)?,
                key_fingerprint: row.get(2)?,
            },
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut targets = Vec::new();
    let mut resolved = 0_i64;
    for row in rows {
        let (target, state) = row?;
        if matches!(state.as_str(), "acknowledged" | "destroyed") {
            resolved = resolved
                .checked_add(1)
                .ok_or(RunnerVolumePurgeError::Conflict)?;
        }
        targets.push(target);
    }
    let required = i64::try_from(targets.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    let target_set_sha256 = runner_target_set_sha256(&targets)?;
    if current.required_target_count != required
        || current.target_set_sha256 != target_set_sha256
        || current.resolved_target_count > resolved
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET resolved_target_count = ?2, updated_at_ms = ?3 \
          WHERE request_id = ?1",
        params![request_id, resolved, now_ms],
    )?;
    load_sqlite_purge_status_tx(tx, request_id)
}

fn reconcile_sqlite_volume_cursor(
    tx: &RunnerSqliteTransaction<'_>,
    volume_id: &str,
    enrollment_epoch: i64,
    now_ms: i64,
) -> RunnerVolumePurgeResult<i64> {
    let required: i64 = tx
        .query_row(
            "SELECT required_tombstone_generation FROM jobs_runner_volumes \
              WHERE volume_id = ?1 AND current_epoch = ?2",
            params![volume_id, enrollment_epoch],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let unsatisfied: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_runner_purge_tombstones s \
           JOIN jobs_runner_purge_requests r ON r.request_id = s.request_id \
          WHERE s.tombstone_generation <= ?3 \
            AND NOT EXISTS ( \
              SELECT 1 FROM jobs_runner_purge_targets t \
               WHERE t.request_id = r.request_id AND t.volume_id = ?1 \
                 AND t.volume_epoch = ?2 AND t.state IN ('acknowledged', 'destroyed') \
            ) \
            AND NOT EXISTS ( \
              SELECT 1 FROM jobs_runner_purge_enforcements e \
               WHERE e.request_id = r.request_id AND e.volume_id = ?1 \
                 AND e.volume_epoch = ?2 AND e.state = 'acknowledged' \
            )",
        params![volume_id, enrollment_epoch, required],
        |row| row.get(0),
    )?;
    if unsatisfied == 0 {
        tx.execute(
            "UPDATE jobs_runner_volumes \
                SET reconciled_tombstone_generation = required_tombstone_generation, \
                    updated_at_ms = ?3 \
              WHERE volume_id = ?1 AND current_epoch = ?2",
            params![volume_id, enrollment_epoch, now_ms],
        )?;
        return Ok(required);
    }
    tx.query_row(
        "SELECT reconciled_tombstone_generation FROM jobs_runner_volumes \
          WHERE volume_id = ?1 AND current_epoch = ?2",
        params![volume_id, enrollment_epoch],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn acknowledge_runner_volume_purge_postgres(
    pool: &DbPool,
    key_ring: &RunnerPurgeCommandKeyRing,
    ack: &RunnerPurgeAck,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeAckOutcome> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.query_one(
        "SELECT singleton_id FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let volume = load_postgres_ack_volume(&mut tx, ack)?;
    verify_runner_purge_ack(ack, &volume.4)?;
    let stored = load_postgres_purge_command_for_ack(&mut tx, key_ring, ack)?;
    validate_ack_against_command(ack, &stored.command, &stored.command_sha256)?;
    let ack_json = serde_json::to_string(ack).map_err(anyhow::Error::from)?;
    let ack_sha256 = ack.ack_sha256()?;
    if stored.state == "acknowledged" {
        if stored.ack_json.as_deref() != Some(ack_json.as_str())
            || stored.ack_sha256.as_deref() != Some(ack_sha256.as_str())
            || stored.ack_signature.as_deref() != Some(ack.signature.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        let status = load_postgres_purge_status_tx(&mut tx, &ack.request_id)?;
        tx.commit()?;
        return Ok(RunnerPurgeAckOutcome {
            disposition: RunnerVolumeWriteDisposition::Replay,
            status,
            reconciled_tombstone_generation: volume.3,
        });
    }
    if stored.state != "pending"
        || (stored.kind == StoredPurgeCommandKind::Target
            && stored.request_state.as_deref() != Some("pending"))
        || volume.0 != ack.enrollment_epoch
        || volume.1 == "destroyed"
        || volume.2.as_deref() != Some(ack.process_instance_id.as_str())
        || volume.5.is_none_or(|expiry| expiry <= received_at_ms)
    {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    update_postgres_stored_ack(
        &mut tx,
        stored.kind,
        ack,
        &ack_json,
        &ack_sha256,
        received_at_ms,
    )?;
    record_postgres_purged_residency(&mut tx, ack, received_at_ms)?;
    let status = recompute_postgres_purge_request(&mut tx, &ack.request_id, received_at_ms)?;
    let reconciled = reconcile_postgres_volume_cursor(
        &mut tx,
        &ack.target_volume_id,
        ack.enrollment_epoch,
        received_at_ms,
    )?;
    tx.commit()?;
    Ok(RunnerPurgeAckOutcome {
        disposition: RunnerVolumeWriteDisposition::Applied,
        status,
        reconciled_tombstone_generation: reconciled,
    })
}

fn load_postgres_ack_volume(
    tx: &mut postgres::Transaction<'_>,
    ack: &RunnerPurgeAck,
) -> RunnerVolumePurgeResult<RunnerAckVolume> {
    let row = tx
        .query_opt(
            "SELECT v.current_epoch, v.status, v.active_instance_id, \
                    v.reconciled_tombstone_generation, k.public_key_base64url, \
                    v.instance_lease_expires_at_ms, k.key_fingerprint \
               FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = $2 \
              WHERE v.volume_id = $1 FOR UPDATE OF v, k",
            &[&ack.target_volume_id, &ack.enrollment_epoch],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let key_fingerprint: String = row.get(6);
    if key_fingerprint != ack.target_key_fingerprint {
        return Err(RunnerVolumePurgeError::Unauthorized);
    }
    Ok((
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        row.get(5),
    ))
}

fn load_postgres_purge_command_for_ack(
    tx: &mut postgres::Transaction<'_>,
    key_ring: &RunnerPurgeCommandKeyRing,
    ack: &RunnerPurgeAck,
) -> RunnerVolumePurgeResult<StoredPurgeCommandForAck> {
    for (table, kind) in [
        ("jobs_runner_purge_targets", StoredPurgeCommandKind::Target),
        (
            "jobs_runner_purge_enforcements",
            StoredPurgeCommandKind::Enforcement,
        ),
    ] {
        let source = if table == "jobs_runner_purge_targets" {
            "jobs_runner_purge_targets AS command \
             JOIN jobs_runner_purge_requests AS request \
               ON request.request_id = command.request_id"
        } else {
            "jobs_runner_purge_enforcements AS command"
        };
        let request_state_column = if table == "jobs_runner_purge_targets" {
            "request.state"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT command.state, command.command_json, command.command_sha256, \
                    command.server_key_id, command.server_signature, command.ack_json, \
                    command.ack_sha256, command.ack_signature, {request_state_column} \
               FROM {source} \
              WHERE command.request_id = $1 AND command.volume_id = $2 \
                AND command.volume_epoch = $3 AND command.command_id = $4 \
              FOR UPDATE OF command"
        );
        let Some(row) = tx.query_opt(
            &sql,
            &[
                &ack.request_id,
                &ack.target_volume_id,
                &ack.enrollment_epoch,
                &ack.command_id,
            ],
        )?
        else {
            continue;
        };
        let command_json: String = row.get(1);
        let command_sha256: String = row.get(2);
        let server_key_id: String = row.get(3);
        let server_signature: String = row.get(4);
        let command = parse_stored_runner_purge_command(
            &command_json,
            &command_sha256,
            &server_key_id,
            &server_signature,
            key_ring,
        )?;
        return Ok(StoredPurgeCommandForAck {
            kind,
            state: row.get(0),
            request_state: row.get(8),
            command,
            command_sha256,
            ack_json: row.get(5),
            ack_sha256: row.get(6),
            ack_signature: row.get(7),
        });
    }
    Err(RunnerVolumePurgeError::NotFound)
}

fn update_postgres_stored_ack(
    tx: &mut postgres::Transaction<'_>,
    kind: StoredPurgeCommandKind,
    ack: &RunnerPurgeAck,
    ack_json: &str,
    ack_sha256: &str,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    let table = match kind {
        StoredPurgeCommandKind::Target => "jobs_runner_purge_targets",
        StoredPurgeCommandKind::Enforcement => "jobs_runner_purge_enforcements",
    };
    let sql = format!(
        "UPDATE {table} \
            SET state = 'acknowledged', ack_json = $5, ack_sha256 = $6, \
                ack_signature = $7, ack_process_instance_id = $8, \
                ack_command_sha256 = $9, ack_inventory_before_count = $10, \
                ack_inventory_before_sha256 = $11, ack_inventory_after_count = $12, \
                ack_inventory_after_sha256 = $13, ack_removed_entry_count = $14, \
                ack_runner_build_id = $15, ack_completed_at_ms = $16, \
                updated_at_ms = $17, acknowledged_at_ms = $17 \
          WHERE request_id = $1 AND volume_id = $2 AND volume_epoch = $3 \
            AND command_id = $4 AND state = 'pending'"
    );
    if tx.execute(
        &sql,
        &[
            &ack.request_id,
            &ack.target_volume_id,
            &ack.enrollment_epoch,
            &ack.command_id,
            &ack_json,
            &ack_sha256,
            &ack.signature,
            &ack.process_instance_id,
            &ack.command_sha256,
            &ack.before_inventory_count,
            &ack.before_inventory_sha256,
            &ack.after_inventory_count,
            &ack.after_inventory_sha256,
            &ack.removed_count,
            &ack.runner_build_id,
            &ack.completed_at_ms,
            &received_at_ms,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn record_postgres_purged_residency(
    tx: &mut postgres::Transaction<'_>,
    ack: &RunnerPurgeAck,
    received_at_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    let purge_subject: String = tx
        .query_opt(
            "SELECT purge_subject FROM jobs_runner_purge_requests \
              WHERE request_id = $1 FOR UPDATE",
            &[&ack.request_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?
        .get(0);
    if runner_purge_subject_sha256(&purge_subject)? != ack.purge_subject_sha256 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let existing = tx.query_opt(
        "SELECT state, purge_generation, first_recorded_at_ms \
           FROM jobs_runner_volume_residencies \
          WHERE purge_subject = $1 AND volume_id = $2 AND volume_epoch = $3 \
          FOR UPDATE",
        &[&purge_subject, &ack.target_volume_id, &ack.enrollment_epoch],
    )?;
    if existing
        .as_ref()
        .is_some_and(|row| row.get::<_, i64>(1) > ack.purge_generation)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let first_recorded_at_ms = existing.map(|row| row.get(2)).unwrap_or(received_at_ms);
    tx.execute(
        "INSERT INTO jobs_runner_volume_residencies ( \
            purge_subject, volume_id, volume_epoch, state, purge_generation, \
            first_recorded_at_ms, last_recorded_at_ms \
         ) VALUES ($1, $2, $3, 'purged', $4, $5, $6) \
         ON CONFLICT(purge_subject, volume_id, volume_epoch) DO UPDATE SET \
            state = 'purged', purge_generation = EXCLUDED.purge_generation, \
            last_recorded_at_ms = EXCLUDED.last_recorded_at_ms",
        &[
            &purge_subject,
            &ack.target_volume_id,
            &ack.enrollment_epoch,
            &ack.purge_generation,
            &first_recorded_at_ms,
            &received_at_ms,
        ],
    )?;
    Ok(())
}

fn load_postgres_purge_status_tx(
    tx: &mut postgres::Transaction<'_>,
    request_id: &str,
) -> RunnerVolumePurgeResult<RunnerPurgeRequestStatus> {
    let select = format!(
        "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
           FROM jobs_runner_purge_requests WHERE request_id = $1 FOR UPDATE"
    );
    tx.query_opt(&select, &[&request_id])?
        .map(purge_request_status_from_pg_row)
        .ok_or(RunnerVolumePurgeError::NotFound)
}

fn recompute_postgres_purge_request(
    tx: &mut postgres::Transaction<'_>,
    request_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeRequestStatus> {
    let current = load_postgres_purge_status_tx(tx, request_id)?;
    let rows = tx.query(
        "SELECT volume_id, volume_epoch, volume_key_fingerprint, state \
           FROM jobs_runner_purge_targets WHERE request_id = $1 \
          ORDER BY volume_id, volume_epoch FOR UPDATE",
        &[&request_id],
    )?;
    let mut targets = Vec::with_capacity(rows.len());
    let mut resolved = 0_i64;
    for row in rows {
        let state: String = row.get(3);
        if matches!(state.as_str(), "acknowledged" | "destroyed") {
            resolved = resolved
                .checked_add(1)
                .ok_or(RunnerVolumePurgeError::Conflict)?;
        }
        targets.push(RunnerPurgeTargetIdentity {
            volume_id: row.get(0),
            enrollment_epoch: row.get(1),
            key_fingerprint: row.get(2),
        });
    }
    let required = i64::try_from(targets.len()).map_err(|_| RunnerVolumePurgeError::Conflict)?;
    let target_set_sha256 = runner_target_set_sha256(&targets)?;
    if current.required_target_count != required
        || current.target_set_sha256 != target_set_sha256
        || current.resolved_target_count > resolved
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET resolved_target_count = $2, updated_at_ms = $3 \
          WHERE request_id = $1",
        &[&request_id, &resolved, &now_ms],
    )?;
    load_postgres_purge_status_tx(tx, request_id)
}

fn reconcile_postgres_volume_cursor(
    tx: &mut postgres::Transaction<'_>,
    volume_id: &str,
    enrollment_epoch: i64,
    now_ms: i64,
) -> RunnerVolumePurgeResult<i64> {
    let required: i64 = tx
        .query_opt(
            "SELECT required_tombstone_generation FROM jobs_runner_volumes \
              WHERE volume_id = $1 AND current_epoch = $2 FOR UPDATE",
            &[&volume_id, &enrollment_epoch],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?
        .get(0);
    let unsatisfied: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_runner_purge_tombstones s \
               JOIN jobs_runner_purge_requests r ON r.request_id = s.request_id \
              WHERE s.tombstone_generation <= $3 \
                AND NOT EXISTS ( \
                  SELECT 1 FROM jobs_runner_purge_targets t \
                   WHERE t.request_id = r.request_id AND t.volume_id = $1 \
                     AND t.volume_epoch = $2 AND t.state IN ('acknowledged', 'destroyed') \
                ) \
                AND NOT EXISTS ( \
                  SELECT 1 FROM jobs_runner_purge_enforcements e \
                   WHERE e.request_id = r.request_id AND e.volume_id = $1 \
                     AND e.volume_epoch = $2 AND e.state = 'acknowledged' \
                )",
            &[&volume_id, &enrollment_epoch, &required],
        )?
        .get(0);
    if unsatisfied == 0 {
        tx.execute(
            "UPDATE jobs_runner_volumes \
                SET reconciled_tombstone_generation = required_tombstone_generation, \
                    updated_at_ms = $3 \
              WHERE volume_id = $1 AND current_epoch = $2",
            &[&volume_id, &enrollment_epoch, &now_ms],
        )?;
        return Ok(required);
    }
    Ok(tx
        .query_one(
            "SELECT reconciled_tombstone_generation FROM jobs_runner_volumes \
              WHERE volume_id = $1 AND current_epoch = $2",
            &[&volume_id, &enrollment_epoch],
        )?
        .get(0))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveRunnerPurgeLegacyRequest {
    pub request_id: String,
    pub expected_legacy_unresolved_count: i64,
    pub resolution_ref: String,
    pub resolution_sha256: String,
    pub resolved_by: String,
    pub resolved_at_ms: i64,
}

pub fn resolve_runner_volume_purge_legacy(
    pool: &DbPool,
    input: &ResolveRunnerPurgeLegacyRequest,
) -> RunnerVolumePurgeResult<(RunnerVolumeWriteDisposition, RunnerPurgeRequestStatus)> {
    require_runner_identifier(&input.request_id)?;
    require_nonempty_text(&input.resolution_ref, 1_024)?;
    require_sha256(&input.resolution_sha256)?;
    require_nonempty_text(&input.resolved_by, 240)?;
    if input.expected_legacy_unresolved_count <= 0 || input.resolved_at_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let legacy_generation: i64 = tx.query_row(
                "SELECT legacy_reconciliation_generation \
                   FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            let row = tx
                .query_row(
                    "SELECT legacy_unresolved_count, legacy_resolution_ref, \
                            legacy_resolution_sha256, legacy_resolved_by, legacy_resolved_at_ms \
                       FROM jobs_runner_purge_requests WHERE request_id = ?1",
                    params![input.request_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<i64>>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            let exact = row.0 == 0
                && row.1.as_deref() == Some(input.resolution_ref.as_str())
                && row.2.as_deref() == Some(input.resolution_sha256.as_str())
                && row.3.as_deref() == Some(input.resolved_by.as_str())
                && row.4.is_some();
            if exact {
                let status = load_sqlite_purge_status_tx(&tx, &input.request_id)?;
                tx.commit()?;
                return Ok((RunnerVolumeWriteDisposition::Replay, status));
            }
            if row.0 == 0 || row.1.is_some() {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            if row.0 != input.expected_legacy_unresolved_count {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            let next_legacy_generation = legacy_generation
                .checked_add(1)
                .ok_or(RunnerVolumePurgeError::Conflict)?;
            invalidate_sqlite_runner_fleet_cutover(&tx, input.resolved_at_ms)?;
            tx.execute(
                "UPDATE jobs_runner_purge_requests \
                    SET legacy_unresolved_count = 0, legacy_resolution_ref = ?2, \
                        legacy_resolution_sha256 = ?3, legacy_resolved_by = ?4, \
                        legacy_resolved_at_ms = ?5, updated_at_ms = ?5 \
                  WHERE request_id = ?1 AND state = 'pending'",
                params![
                    input.request_id,
                    input.resolution_ref,
                    input.resolution_sha256,
                    input.resolved_by,
                    input.resolved_at_ms,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_runner_volume_fleet_state \
                    SET legacy_reconciliation_generation = ?1, updated_at_ms = ?2 \
                  WHERE singleton_id = 1 AND legacy_reconciliation_generation = ?3",
                params![
                    next_legacy_generation,
                    input.resolved_at_ms,
                    legacy_generation,
                ],
            )?;
            let status = load_sqlite_purge_status_tx(&tx, &input.request_id)?;
            tx.commit()?;
            Ok((RunnerVolumeWriteDisposition::Applied, status))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let legacy_generation: i64 = tx
                .query_one(
                    "SELECT legacy_reconciliation_generation FROM jobs_runner_volume_fleet_state \
                  WHERE singleton_id = 1 FOR UPDATE",
                    &[],
                )?
                .get(0);
            let row = tx
                .query_opt(
                    "SELECT legacy_unresolved_count, legacy_resolution_ref, \
                            legacy_resolution_sha256, legacy_resolved_by, legacy_resolved_at_ms \
                       FROM jobs_runner_purge_requests WHERE request_id = $1 FOR UPDATE",
                    &[&input.request_id],
                )?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            let count: i64 = row.get(0);
            let reference: Option<String> = row.get(1);
            let sha256: Option<String> = row.get(2);
            let resolved_by: Option<String> = row.get(3);
            let resolved_at_ms: Option<i64> = row.get(4);
            let exact = count == 0
                && reference.as_deref() == Some(input.resolution_ref.as_str())
                && sha256.as_deref() == Some(input.resolution_sha256.as_str())
                && resolved_by.as_deref() == Some(input.resolved_by.as_str())
                && resolved_at_ms.is_some();
            if exact {
                let status = load_postgres_purge_status_tx(&mut tx, &input.request_id)?;
                tx.commit()?;
                return Ok((RunnerVolumeWriteDisposition::Replay, status));
            }
            if count == 0 || reference.is_some() {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            if count != input.expected_legacy_unresolved_count {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            let next_legacy_generation = legacy_generation
                .checked_add(1)
                .ok_or(RunnerVolumePurgeError::Conflict)?;
            invalidate_postgres_runner_fleet_cutover(&mut tx, input.resolved_at_ms)?;
            tx.execute(
                "UPDATE jobs_runner_purge_requests \
                    SET legacy_unresolved_count = 0, legacy_resolution_ref = $2, \
                        legacy_resolution_sha256 = $3, legacy_resolved_by = $4, \
                        legacy_resolved_at_ms = $5, updated_at_ms = $5 \
                  WHERE request_id = $1 AND state = 'pending'",
                &[
                    &input.request_id,
                    &input.resolution_ref,
                    &input.resolution_sha256,
                    &input.resolved_by,
                    &input.resolved_at_ms,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_runner_volume_fleet_state \
                    SET legacy_reconciliation_generation = $1, updated_at_ms = $2 \
                  WHERE singleton_id = 1 AND legacy_reconciliation_generation = $3",
                &[
                    &next_legacy_generation,
                    &input.resolved_at_ms,
                    &legacy_generation,
                ],
            )?;
            let status = load_postgres_purge_status_tx(&mut tx, &input.request_id)?;
            tx.commit()?;
            Ok((RunnerVolumeWriteDisposition::Applied, status))
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordRunnerVolumeFleetCutoverRequest {
    pub cutover_state: String,
    pub expected_enrollment_generation: i64,
    pub expected_purge_generation: i64,
    pub expected_tombstone_generation: i64,
    pub expected_destruction_generation: i64,
    pub expected_legacy_reconciliation_generation: i64,
    pub expected_storage_attestation_generation: i64,
    pub expected_storage_attestation_count: i64,
    pub expected_storage_attestation_set_sha256: String,
    pub expected_legacy_inventory_generation: i64,
    pub expected_legacy_inventory_reconciliation_id: String,
    pub expected_legacy_inventory_authority_id: String,
    pub expected_legacy_inventory_authority_sha256: String,
    pub expected_legacy_inventory_root_count: i64,
    pub expected_legacy_inventory_root_set_sha256: String,
    pub expected_non_destroyed_volume_count: i64,
    pub expected_destruction_count: i64,
    pub expected_unresolved_legacy_volume_count: i64,
    pub evidence_ref: String,
    pub evidence_sha256: String,
    pub authorized_by: String,
    pub cutover_at_ms: i64,
    pub now_ms: i64,
}

pub fn record_runner_volume_fleet_cutover(
    pool: &DbPool,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    if !matches!(input.cutover_state.as_str(), "reconciling" | "ready")
        || input.expected_enrollment_generation < 0
        || input.expected_purge_generation < 0
        || input.expected_tombstone_generation < 0
        || input.expected_destruction_generation < 0
        || input.expected_legacy_reconciliation_generation < 0
        || input.expected_storage_attestation_generation < 0
        || input.expected_storage_attestation_count < 0
        || input.expected_legacy_inventory_generation <= 0
        || input.expected_legacy_inventory_root_count < 0
        || input.expected_non_destroyed_volume_count < 0
        || input.expected_destruction_count < 0
        || input.expected_unresolved_legacy_volume_count < 0
        || input.cutover_at_ms < 0
        || input.now_ms < input.cutover_at_ms
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    require_nonempty_text(&input.evidence_ref, 1_024)?;
    require_sha256(&input.evidence_sha256)?;
    require_nonempty_text(&input.authorized_by, 240)?;
    require_runner_identifier(&input.expected_legacy_inventory_reconciliation_id)?;
    require_runner_identifier(&input.expected_legacy_inventory_authority_id)?;
    require_sha256(&input.expected_legacy_inventory_authority_sha256)?;
    require_sha256(&input.expected_legacy_inventory_root_set_sha256)?;
    require_sha256(&input.expected_storage_attestation_set_sha256)?;
    if input.expected_storage_attestation_count == 0
        && input.expected_storage_attestation_set_sha256
            != EMPTY_RUNNER_STORAGE_ATTESTATION_SET_SHA256
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    if input.expected_legacy_inventory_root_count == 0
        && input.expected_legacy_inventory_root_set_sha256 != EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_runner_volume_fleet_cutover_sqlite(pool, input),
        DbPool::Postgres(_) => record_runner_volume_fleet_cutover_postgres(pool, input),
    })
}

#[derive(Debug)]
struct FleetCutoverRow {
    enrollment_generation: i64,
    purge_generation: i64,
    tombstone_generation: i64,
    destruction_generation: i64,
    legacy_reconciliation_generation: i64,
    storage_attestation_generation: i64,
    storage_attestation_count: i64,
    storage_attestation_set_sha256: String,
    legacy_inventory_state: String,
    legacy_inventory_generation: i64,
    legacy_inventory_reconciliation_id: Option<String>,
    legacy_inventory_authority_id: Option<String>,
    legacy_inventory_authority_sha256: Option<String>,
    legacy_inventory_root_count: Option<i64>,
    legacy_inventory_root_set_sha256: Option<String>,
    cutover_state: String,
    unresolved_legacy_volume_count: i64,
    cutover_enrollment_generation: Option<i64>,
    cutover_purge_generation: Option<i64>,
    cutover_tombstone_generation: Option<i64>,
    cutover_destruction_generation: Option<i64>,
    cutover_legacy_reconciliation_generation: Option<i64>,
    cutover_storage_attestation_generation: Option<i64>,
    cutover_storage_attestation_count: Option<i64>,
    cutover_storage_attestation_set_sha256: Option<String>,
    cutover_legacy_inventory_generation: Option<i64>,
    cutover_legacy_inventory_reconciliation_id: Option<String>,
    cutover_legacy_inventory_authority_id: Option<String>,
    cutover_legacy_inventory_authority_sha256: Option<String>,
    cutover_legacy_inventory_root_count: Option<i64>,
    cutover_legacy_inventory_root_set_sha256: Option<String>,
    cutover_non_destroyed_volume_count: Option<i64>,
    cutover_destruction_count: Option<i64>,
    cutover_unresolved_legacy_volume_count: Option<i64>,
    cutover_evidence_ref: Option<String>,
    cutover_evidence_sha256: Option<String>,
    cutover_authorized_by: Option<String>,
    cutover_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct FleetObservedCounts {
    non_destroyed_volume_count: i64,
    destruction_count: i64,
    unready_volume_count: i64,
}

fn cutover_input_matches_live(
    row: &FleetCutoverRow,
    counts: FleetObservedCounts,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> bool {
    row.enrollment_generation == input.expected_enrollment_generation
        && row.purge_generation == input.expected_purge_generation
        && row.tombstone_generation == input.expected_tombstone_generation
        && row.destruction_generation == input.expected_destruction_generation
        && row.legacy_reconciliation_generation == input.expected_legacy_reconciliation_generation
        && row.storage_attestation_generation == input.expected_storage_attestation_generation
        && row.storage_attestation_count == input.expected_storage_attestation_count
        && row.storage_attestation_set_sha256 == input.expected_storage_attestation_set_sha256
        && row.legacy_inventory_state == "ready"
        && row.legacy_inventory_generation == input.expected_legacy_inventory_generation
        && row.legacy_inventory_reconciliation_id.as_deref()
            == Some(input.expected_legacy_inventory_reconciliation_id.as_str())
        && row.legacy_inventory_authority_id.as_deref()
            == Some(input.expected_legacy_inventory_authority_id.as_str())
        && row.legacy_inventory_authority_sha256.as_deref()
            == Some(input.expected_legacy_inventory_authority_sha256.as_str())
        && row.legacy_inventory_root_count == Some(input.expected_legacy_inventory_root_count)
        && row.legacy_inventory_root_set_sha256.as_deref()
            == Some(input.expected_legacy_inventory_root_set_sha256.as_str())
        && row.unresolved_legacy_volume_count == input.expected_unresolved_legacy_volume_count
        && counts.non_destroyed_volume_count == input.expected_non_destroyed_volume_count
        && counts.destruction_count == input.expected_destruction_count
}

fn stored_cutover_snapshot_matches_input(
    row: &FleetCutoverRow,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> bool {
    row.cutover_enrollment_generation == Some(input.expected_enrollment_generation)
        && row.cutover_purge_generation == Some(input.expected_purge_generation)
        && row.cutover_tombstone_generation == Some(input.expected_tombstone_generation)
        && row.cutover_destruction_generation == Some(input.expected_destruction_generation)
        && row.cutover_legacy_reconciliation_generation
            == Some(input.expected_legacy_reconciliation_generation)
        && row.cutover_storage_attestation_generation
            == Some(input.expected_storage_attestation_generation)
        && row.cutover_storage_attestation_count == Some(input.expected_storage_attestation_count)
        && row.cutover_storage_attestation_set_sha256.as_deref()
            == Some(input.expected_storage_attestation_set_sha256.as_str())
        && row.cutover_legacy_inventory_generation
            == Some(input.expected_legacy_inventory_generation)
        && row.cutover_legacy_inventory_reconciliation_id.as_deref()
            == Some(input.expected_legacy_inventory_reconciliation_id.as_str())
        && row.cutover_legacy_inventory_authority_id.as_deref()
            == Some(input.expected_legacy_inventory_authority_id.as_str())
        && row.cutover_legacy_inventory_authority_sha256.as_deref()
            == Some(input.expected_legacy_inventory_authority_sha256.as_str())
        && row.cutover_legacy_inventory_root_count
            == Some(input.expected_legacy_inventory_root_count)
        && row.cutover_legacy_inventory_root_set_sha256.as_deref()
            == Some(input.expected_legacy_inventory_root_set_sha256.as_str())
        && row.cutover_non_destroyed_volume_count == Some(input.expected_non_destroyed_volume_count)
        && row.cutover_destruction_count == Some(input.expected_destruction_count)
        && row.cutover_unresolved_legacy_volume_count
            == Some(input.expected_unresolved_legacy_volume_count)
        && row.cutover_evidence_ref.as_deref() == Some(input.evidence_ref.as_str())
        && row.cutover_evidence_sha256.as_deref() == Some(input.evidence_sha256.as_str())
        && row.cutover_authorized_by.as_deref() == Some(input.authorized_by.as_str())
        // `cutover_at_ms` is server allocated rather than caller evidence. Two
        // exact concurrent phase-one requests can mint different wall-clock
        // values before either acquires the database lock; the locked winner's
        // timestamp remains authoritative for replay and phase two.
        && row.cutover_at_ms.is_some()
}

fn has_no_cutover_snapshot(row: &FleetCutoverRow) -> bool {
    row.cutover_enrollment_generation.is_none()
        && row.cutover_purge_generation.is_none()
        && row.cutover_tombstone_generation.is_none()
        && row.cutover_destruction_generation.is_none()
        && row.cutover_legacy_reconciliation_generation.is_none()
        && row.cutover_storage_attestation_generation.is_none()
        && row.cutover_storage_attestation_count.is_none()
        && row.cutover_storage_attestation_set_sha256.is_none()
        && row.cutover_legacy_inventory_generation.is_none()
        && row.cutover_legacy_inventory_reconciliation_id.is_none()
        && row.cutover_legacy_inventory_authority_id.is_none()
        && row.cutover_legacy_inventory_authority_sha256.is_none()
        && row.cutover_legacy_inventory_root_count.is_none()
        && row.cutover_legacy_inventory_root_set_sha256.is_none()
        && row.cutover_non_destroyed_volume_count.is_none()
        && row.cutover_destruction_count.is_none()
        && row.cutover_unresolved_legacy_volume_count.is_none()
        && row.cutover_evidence_ref.is_none()
        && row.cutover_evidence_sha256.is_none()
        && row.cutover_authorized_by.is_none()
        && row.cutover_at_ms.is_none()
}

fn validate_cutover_state_change(
    row: &FleetCutoverRow,
    counts: FleetObservedCounts,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> RunnerVolumePurgeResult<Option<RunnerVolumeWriteDisposition>> {
    if !cutover_input_matches_live(row, counts, input) {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    if row.cutover_state == input.cutover_state && stored_cutover_snapshot_matches_input(row, input)
    {
        if input.cutover_state == "ready"
            && (counts.unready_volume_count != 0
                || input.expected_unresolved_legacy_volume_count != 0)
        {
            return Err(RunnerVolumePurgeError::NotReady);
        }
        return Ok(Some(RunnerVolumeWriteDisposition::Replay));
    }
    match input.cutover_state.as_str() {
        "reconciling"
            if matches!(row.cutover_state.as_str(), "pre_cutover" | "reconciling")
                && has_no_cutover_snapshot(row) =>
        {
            Ok(None)
        }
        "ready"
            if row.cutover_state == "reconciling"
                && stored_cutover_snapshot_matches_input(row, input)
                && counts.unready_volume_count == 0
                && input.expected_unresolved_legacy_volume_count == 0 =>
        {
            Ok(None)
        }
        "ready" => Err(RunnerVolumePurgeError::NotReady),
        _ => Err(RunnerVolumePurgeError::Conflict),
    }
}

fn record_runner_volume_fleet_cutover_sqlite(
    pool: &DbPool,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let row: FleetCutoverRow = tx.query_row(
        "SELECT enrollment_generation, purge_generation, tombstone_generation, \
                destruction_generation, legacy_reconciliation_generation, \
                legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256, legacy_inventory_root_count, \
                legacy_inventory_root_set_sha256, cutover_state, \
                unresolved_legacy_volume_count, cutover_enrollment_generation, \
                cutover_purge_generation, cutover_tombstone_generation, \
                cutover_destruction_generation, cutover_legacy_reconciliation_generation, \
                cutover_legacy_inventory_generation, \
                cutover_legacy_inventory_reconciliation_id, \
                cutover_legacy_inventory_authority_id, \
                cutover_legacy_inventory_authority_sha256, \
                cutover_legacy_inventory_root_count, \
                cutover_legacy_inventory_root_set_sha256, \
                cutover_non_destroyed_volume_count, cutover_destruction_count, \
                cutover_unresolved_legacy_volume_count, cutover_evidence_ref, \
                cutover_evidence_sha256, cutover_authorized_by, cutover_at_ms, \
                storage_attestation_generation, storage_attestation_count, \
                storage_attestation_set_sha256, cutover_storage_attestation_generation, \
                cutover_storage_attestation_count, cutover_storage_attestation_set_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| {
            Ok(FleetCutoverRow {
                enrollment_generation: row.get(0)?,
                purge_generation: row.get(1)?,
                tombstone_generation: row.get(2)?,
                destruction_generation: row.get(3)?,
                legacy_reconciliation_generation: row.get(4)?,
                legacy_inventory_state: row.get(5)?,
                legacy_inventory_generation: row.get(6)?,
                legacy_inventory_reconciliation_id: row.get(7)?,
                legacy_inventory_authority_id: row.get(8)?,
                legacy_inventory_authority_sha256: row.get(9)?,
                legacy_inventory_root_count: row.get(10)?,
                legacy_inventory_root_set_sha256: row.get(11)?,
                cutover_state: row.get(12)?,
                unresolved_legacy_volume_count: row.get(13)?,
                cutover_enrollment_generation: row.get(14)?,
                cutover_purge_generation: row.get(15)?,
                cutover_tombstone_generation: row.get(16)?,
                cutover_destruction_generation: row.get(17)?,
                cutover_legacy_reconciliation_generation: row.get(18)?,
                cutover_legacy_inventory_generation: row.get(19)?,
                cutover_legacy_inventory_reconciliation_id: row.get(20)?,
                cutover_legacy_inventory_authority_id: row.get(21)?,
                cutover_legacy_inventory_authority_sha256: row.get(22)?,
                cutover_legacy_inventory_root_count: row.get(23)?,
                cutover_legacy_inventory_root_set_sha256: row.get(24)?,
                cutover_non_destroyed_volume_count: row.get(25)?,
                cutover_destruction_count: row.get(26)?,
                cutover_unresolved_legacy_volume_count: row.get(27)?,
                cutover_evidence_ref: row.get(28)?,
                cutover_evidence_sha256: row.get(29)?,
                cutover_authorized_by: row.get(30)?,
                cutover_at_ms: row.get(31)?,
                storage_attestation_generation: row.get(32)?,
                storage_attestation_count: row.get(33)?,
                storage_attestation_set_sha256: row.get(34)?,
                cutover_storage_attestation_generation: row.get(35)?,
                cutover_storage_attestation_count: row.get(36)?,
                cutover_storage_attestation_set_sha256: row.get(37)?,
            })
        },
    )?;
    let counts = tx.query_row(
        "SELECT \
            (SELECT COUNT(*) FROM jobs_runner_volumes WHERE status <> 'destroyed'), \
            (SELECT COUNT(*) FROM jobs_runner_volume_destructions), \
            (SELECT COUNT(*) FROM jobs_runner_volumes \
              WHERE status <> 'destroyed' AND (status NOT IN ('active', 'suspended', 'retired') \
                 OR required_tombstone_generation <> reconciled_tombstone_generation \
                 OR NOT EXISTS (SELECT 1 \
                   FROM jobs_runner_volume_storage_attestations a \
                   JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = jobs_runner_volumes.volume_id \
                    AND k.enrollment_epoch = jobs_runner_volumes.current_epoch \
                  WHERE a.volume_id = jobs_runner_volumes.volume_id \
                    AND a.enrollment_epoch = jobs_runner_volumes.current_epoch \
                    AND a.volume_key_fingerprint = k.key_fingerprint \
                    AND a.process_instance_id = jobs_runner_volumes.active_instance_id \
                    AND a.enrollment_generation = \
                        jobs_runner_volumes.enrollment_generation \
                    AND a.required_tombstone_generation = \
                        jobs_runner_volumes.required_tombstone_generation \
                    AND a.reconciled_tombstone_generation = \
                        jobs_runner_volumes.reconciled_tombstone_generation \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation))))",
        [],
        |row| {
            Ok(FleetObservedCounts {
                non_destroyed_volume_count: row.get(0)?,
                destruction_count: row.get(1)?,
                unready_volume_count: row.get(2)?,
            })
        },
    )?;
    if validate_cutover_state_change(&row, counts, input)?.is_some() {
        tx.commit()?;
        return Ok(RunnerVolumeWriteDisposition::Replay);
    }
    let updated = if input.cutover_state == "reconciling" {
        tx.execute(
            "UPDATE jobs_runner_volume_fleet_state \
                SET cutover_state = 'reconciling', cutover_enrollment_generation = ?1, \
                    cutover_purge_generation = ?2, cutover_tombstone_generation = ?3, \
                    cutover_destruction_generation = ?4, \
                    cutover_legacy_reconciliation_generation = ?5, \
                    cutover_legacy_inventory_generation = ?6, \
                    cutover_legacy_inventory_reconciliation_id = ?7, \
                    cutover_legacy_inventory_authority_id = ?8, \
                    cutover_legacy_inventory_authority_sha256 = ?9, \
                    cutover_legacy_inventory_root_count = ?10, \
                    cutover_legacy_inventory_root_set_sha256 = ?11, \
                    cutover_non_destroyed_volume_count = ?12, \
                    cutover_destruction_count = ?13, \
                    cutover_unresolved_legacy_volume_count = ?14, cutover_evidence_ref = ?15, \
                    cutover_evidence_sha256 = ?16, cutover_authorized_by = ?17, \
                    cutover_at_ms = ?18, updated_at_ms = ?19, \
                    cutover_storage_attestation_generation = ?20, \
                    cutover_storage_attestation_count = ?21, \
                    cutover_storage_attestation_set_sha256 = ?22 \
              WHERE singleton_id = 1 AND cutover_state IN ('pre_cutover', 'reconciling')",
            params![
                input.expected_enrollment_generation,
                input.expected_purge_generation,
                input.expected_tombstone_generation,
                input.expected_destruction_generation,
                input.expected_legacy_reconciliation_generation,
                input.expected_legacy_inventory_generation,
                input.expected_legacy_inventory_reconciliation_id,
                input.expected_legacy_inventory_authority_id,
                input.expected_legacy_inventory_authority_sha256,
                input.expected_legacy_inventory_root_count,
                input.expected_legacy_inventory_root_set_sha256,
                input.expected_non_destroyed_volume_count,
                input.expected_destruction_count,
                input.expected_unresolved_legacy_volume_count,
                input.evidence_ref,
                input.evidence_sha256,
                input.authorized_by,
                input.cutover_at_ms,
                input.now_ms,
                input.expected_storage_attestation_generation,
                input.expected_storage_attestation_count,
                input.expected_storage_attestation_set_sha256,
            ],
        )?
    } else {
        tx.execute(
            "UPDATE jobs_runner_volume_fleet_state \
                SET cutover_state = 'ready', updated_at_ms = ?1 \
              WHERE singleton_id = 1 AND cutover_state = 'reconciling'",
            params![input.now_ms],
        )?
    };
    if updated != 1 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeWriteDisposition::Applied)
}

fn record_runner_volume_fleet_cutover_postgres(
    pool: &DbPool,
    input: &RecordRunnerVolumeFleetCutoverRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let stored = tx.query_one(
        "SELECT enrollment_generation, purge_generation, tombstone_generation, \
                destruction_generation, legacy_reconciliation_generation, \
                legacy_inventory_state, legacy_inventory_generation, \
                legacy_inventory_reconciliation_id, legacy_inventory_authority_id, \
                legacy_inventory_authority_sha256, legacy_inventory_root_count, \
                legacy_inventory_root_set_sha256, cutover_state, \
                unresolved_legacy_volume_count, cutover_enrollment_generation, \
                cutover_purge_generation, cutover_tombstone_generation, \
                cutover_destruction_generation, cutover_legacy_reconciliation_generation, \
                cutover_legacy_inventory_generation, \
                cutover_legacy_inventory_reconciliation_id, \
                cutover_legacy_inventory_authority_id, \
                cutover_legacy_inventory_authority_sha256, \
                cutover_legacy_inventory_root_count, \
                cutover_legacy_inventory_root_set_sha256, \
                cutover_non_destroyed_volume_count, cutover_destruction_count, \
                cutover_unresolved_legacy_volume_count, cutover_evidence_ref, \
                cutover_evidence_sha256, cutover_authorized_by, cutover_at_ms, \
                storage_attestation_generation, storage_attestation_count, \
                storage_attestation_set_sha256, cutover_storage_attestation_generation, \
                cutover_storage_attestation_count, cutover_storage_attestation_set_sha256 \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let row = FleetCutoverRow {
        enrollment_generation: stored.get(0),
        purge_generation: stored.get(1),
        tombstone_generation: stored.get(2),
        destruction_generation: stored.get(3),
        legacy_reconciliation_generation: stored.get(4),
        legacy_inventory_state: stored.get(5),
        legacy_inventory_generation: stored.get(6),
        legacy_inventory_reconciliation_id: stored.get(7),
        legacy_inventory_authority_id: stored.get(8),
        legacy_inventory_authority_sha256: stored.get(9),
        legacy_inventory_root_count: stored.get(10),
        legacy_inventory_root_set_sha256: stored.get(11),
        cutover_state: stored.get(12),
        unresolved_legacy_volume_count: stored.get(13),
        cutover_enrollment_generation: stored.get(14),
        cutover_purge_generation: stored.get(15),
        cutover_tombstone_generation: stored.get(16),
        cutover_destruction_generation: stored.get(17),
        cutover_legacy_reconciliation_generation: stored.get(18),
        cutover_legacy_inventory_generation: stored.get(19),
        cutover_legacy_inventory_reconciliation_id: stored.get(20),
        cutover_legacy_inventory_authority_id: stored.get(21),
        cutover_legacy_inventory_authority_sha256: stored.get(22),
        cutover_legacy_inventory_root_count: stored.get(23),
        cutover_legacy_inventory_root_set_sha256: stored.get(24),
        cutover_non_destroyed_volume_count: stored.get(25),
        cutover_destruction_count: stored.get(26),
        cutover_unresolved_legacy_volume_count: stored.get(27),
        cutover_evidence_ref: stored.get(28),
        cutover_evidence_sha256: stored.get(29),
        cutover_authorized_by: stored.get(30),
        cutover_at_ms: stored.get(31),
        storage_attestation_generation: stored.get(32),
        storage_attestation_count: stored.get(33),
        storage_attestation_set_sha256: stored.get(34),
        cutover_storage_attestation_generation: stored.get(35),
        cutover_storage_attestation_count: stored.get(36),
        cutover_storage_attestation_set_sha256: stored.get(37),
    };
    let volumes = tx.query(
        "SELECT volume_id, status, legacy_artifact_count, required_tombstone_generation, \
                reconciled_tombstone_generation \
           FROM jobs_runner_volumes ORDER BY volume_id FOR UPDATE",
        &[],
    )?;
    let non_destroyed_volume_count = i64::try_from(
        volumes
            .iter()
            .filter(|volume| volume.get::<_, String>(1) != "destroyed")
            .count(),
    )
    .map_err(|_| RunnerVolumePurgeError::Conflict)?;
    let unready_volume_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_runner_volumes v \
              WHERE v.status <> 'destroyed' \
                AND (v.status NOT IN ('active', 'suspended', 'retired') \
                 OR v.required_tombstone_generation <> v.reconciled_tombstone_generation \
                 OR NOT EXISTS (SELECT 1 \
                   FROM jobs_runner_volume_storage_attestations a \
                   JOIN jobs_runner_volume_keys k \
                     ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
                  WHERE a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                    AND a.volume_key_fingerprint = k.key_fingerprint \
                    AND a.process_instance_id = v.active_instance_id \
                    AND a.enrollment_generation = v.enrollment_generation \
                    AND a.required_tombstone_generation = v.required_tombstone_generation \
                    AND a.reconciled_tombstone_generation = \
                        v.reconciled_tombstone_generation \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation))))",
            &[],
        )?
        .get(0);
    let destructions = tx.query(
        "SELECT destruction_id FROM jobs_runner_volume_destructions \
          ORDER BY destruction_id FOR UPDATE",
        &[],
    )?;
    let counts = FleetObservedCounts {
        non_destroyed_volume_count,
        destruction_count: i64::try_from(destructions.len())
            .map_err(|_| RunnerVolumePurgeError::Conflict)?,
        unready_volume_count,
    };
    if validate_cutover_state_change(&row, counts, input)?.is_some() {
        tx.commit()?;
        return Ok(RunnerVolumeWriteDisposition::Replay);
    }
    let updated = if input.cutover_state == "reconciling" {
        tx.execute(
            "UPDATE jobs_runner_volume_fleet_state \
                SET cutover_state = 'reconciling', cutover_enrollment_generation = $1, \
                    cutover_purge_generation = $2, cutover_tombstone_generation = $3, \
                    cutover_destruction_generation = $4, \
                    cutover_legacy_reconciliation_generation = $5, \
                    cutover_legacy_inventory_generation = $6, \
                    cutover_legacy_inventory_reconciliation_id = $7, \
                    cutover_legacy_inventory_authority_id = $8, \
                    cutover_legacy_inventory_authority_sha256 = $9, \
                    cutover_legacy_inventory_root_count = $10, \
                    cutover_legacy_inventory_root_set_sha256 = $11, \
                    cutover_non_destroyed_volume_count = $12, \
                    cutover_destruction_count = $13, \
                    cutover_unresolved_legacy_volume_count = $14, cutover_evidence_ref = $15, \
                    cutover_evidence_sha256 = $16, cutover_authorized_by = $17, \
                    cutover_at_ms = $18, updated_at_ms = $19, \
                    cutover_storage_attestation_generation = $20, \
                    cutover_storage_attestation_count = $21, \
                    cutover_storage_attestation_set_sha256 = $22 \
              WHERE singleton_id = 1 AND cutover_state IN ('pre_cutover', 'reconciling')",
            &[
                &input.expected_enrollment_generation,
                &input.expected_purge_generation,
                &input.expected_tombstone_generation,
                &input.expected_destruction_generation,
                &input.expected_legacy_reconciliation_generation,
                &input.expected_legacy_inventory_generation,
                &input.expected_legacy_inventory_reconciliation_id,
                &input.expected_legacy_inventory_authority_id,
                &input.expected_legacy_inventory_authority_sha256,
                &input.expected_legacy_inventory_root_count,
                &input.expected_legacy_inventory_root_set_sha256,
                &input.expected_non_destroyed_volume_count,
                &input.expected_destruction_count,
                &input.expected_unresolved_legacy_volume_count,
                &input.evidence_ref,
                &input.evidence_sha256,
                &input.authorized_by,
                &input.cutover_at_ms,
                &input.now_ms,
                &input.expected_storage_attestation_generation,
                &input.expected_storage_attestation_count,
                &input.expected_storage_attestation_set_sha256,
            ],
        )?
    } else {
        tx.execute(
            "UPDATE jobs_runner_volume_fleet_state \
                SET cutover_state = 'ready', updated_at_ms = $1 \
              WHERE singleton_id = 1 AND cutover_state = 'reconciling'",
            &[&input.now_ms],
        )?
    };
    if updated != 1 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeWriteDisposition::Applied)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerPurgeCompletion {
    pub status: RunnerPurgeRequestStatus,
    pub tombstone_generation: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

pub fn complete_runner_volume_purge(
    pool: &DbPool,
    request_id: &str,
    completed_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeCompletion> {
    require_runner_identifier(request_id)?;
    if completed_at_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => complete_runner_volume_purge_sqlite(pool, request_id, completed_at_ms),
        DbPool::Postgres(_) => {
            complete_runner_volume_purge_postgres(pool, request_id, completed_at_ms)
        }
    })
}

fn purge_request_matches_ready_legacy_inventory(
    status: &RunnerPurgeRequestStatus,
    state: &str,
    generation: i64,
    reconciliation_id: Option<&str>,
    authority_id: Option<&str>,
    authority_sha256: Option<&str>,
) -> bool {
    state == "ready"
        && status.legacy_inventory_generation == generation
        && reconciliation_id == Some(status.legacy_inventory_reconciliation_id.as_str())
        && authority_id == Some(status.legacy_inventory_authority_id.as_str())
        && authority_sha256 == Some(status.legacy_inventory_authority_sha256.as_str())
}

fn complete_runner_volume_purge_sqlite(
    pool: &DbPool,
    request_id: &str,
    completed_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeCompletion> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let fleet = tx.query_row(
        "SELECT tombstone_generation, legacy_inventory_state, \
                legacy_inventory_generation, legacy_inventory_reconciliation_id, \
                legacy_inventory_authority_id, legacy_inventory_authority_sha256 \
           FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        },
    )?;
    let current = load_sqlite_purge_status_tx(&tx, request_id)?;
    if !purge_request_matches_ready_legacy_inventory(
        &current,
        &fleet.1,
        fleet.2,
        fleet.3.as_deref(),
        fleet.4.as_deref(),
        fleet.5.as_deref(),
    ) {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    if current.state == "complete" {
        let tombstone_generation = exact_sqlite_completed_tombstone(&tx, &current)?;
        tx.commit()?;
        return Ok(RunnerPurgeCompletion {
            status: current,
            tombstone_generation,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    let status = recompute_sqlite_purge_request(&tx, request_id, completed_at_ms)?;
    if status.legacy_unresolved_count != 0
        || status.resolved_target_count != status.required_target_count
    {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let tombstone_generation = fleet
        .0
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_tombstones ( \
            purge_subject, request_id, purge_generation, tombstone_generation, \
            target_set_sha256, required_target_count, completed_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            status.purge_subject,
            status.request_id,
            status.purge_generation,
            tombstone_generation,
            status.target_set_sha256,
            status.required_target_count,
            completed_at_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET state = 'complete', updated_at_ms = ?2, completed_at_ms = ?2 \
          WHERE request_id = ?1 AND state = 'pending'",
        params![request_id, completed_at_ms],
    )?;
    invalidate_sqlite_runner_fleet_cutover(&tx, completed_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET tombstone_generation = ?1, updated_at_ms = ?2 \
          WHERE singleton_id = 1 AND tombstone_generation = ?3",
        params![tombstone_generation, completed_at_ms, fleet.0,],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let volume_ids = sqlite_non_destroyed_volume_epochs(&tx)?;
    for (volume_id, enrollment_epoch) in volume_ids {
        tx.execute(
            "UPDATE jobs_runner_volumes \
                SET required_tombstone_generation = ?2, \
                    status = CASE WHEN status = 'active' THEN 'reconciling' ELSE status END, \
                    updated_at_ms = ?3 \
              WHERE volume_id = ?1 AND status <> 'destroyed'",
            params![volume_id, tombstone_generation, completed_at_ms],
        )?;
        reconcile_sqlite_volume_cursor(&tx, &volume_id, enrollment_epoch, completed_at_ms)?;
    }
    let completed = load_sqlite_purge_status_tx(&tx, request_id)?;
    tx.commit()?;
    Ok(RunnerPurgeCompletion {
        status: completed,
        tombstone_generation,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn exact_sqlite_completed_tombstone(
    tx: &RunnerSqliteTransaction<'_>,
    status: &RunnerPurgeRequestStatus,
) -> RunnerVolumePurgeResult<i64> {
    let row = tx
        .query_row(
            "SELECT purge_subject, purge_generation, tombstone_generation, \
                    target_set_sha256, required_target_count, completed_at_ms \
               FROM jobs_runner_purge_tombstones WHERE request_id = ?1",
            params![status.request_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    if row.0 != status.purge_subject
        || row.1 != status.purge_generation
        || row.3 != status.target_set_sha256
        || row.4 != status.required_target_count
        || Some(row.5) != status.completed_at_ms
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(row.2)
}

fn sqlite_non_destroyed_volume_epochs(
    tx: &RunnerSqliteTransaction<'_>,
) -> RunnerVolumePurgeResult<Vec<(String, i64)>> {
    let mut statement = tx.prepare(
        "SELECT volume_id, current_epoch FROM jobs_runner_volumes \
          WHERE status <> 'destroyed' ORDER BY volume_id, current_epoch",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn complete_runner_volume_purge_postgres(
    pool: &DbPool,
    request_id: &str,
    completed_at_ms: i64,
) -> RunnerVolumePurgeResult<RunnerPurgeCompletion> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let fleet_row = tx.query_one(
        "SELECT tombstone_generation, legacy_inventory_state, \
                    legacy_inventory_generation, legacy_inventory_reconciliation_id, \
                    legacy_inventory_authority_id, legacy_inventory_authority_sha256 \
               FROM jobs_runner_volume_fleet_state \
              WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let fleet_tombstone_generation: i64 = fleet_row.get(0);
    let fleet_legacy_state: String = fleet_row.get(1);
    let fleet_legacy_generation: i64 = fleet_row.get(2);
    let fleet_legacy_reconciliation_id: Option<String> = fleet_row.get(3);
    let fleet_legacy_authority_id: Option<String> = fleet_row.get(4);
    let fleet_legacy_authority_sha256: Option<String> = fleet_row.get(5);
    let unlocked_request = {
        let select = format!(
            "SELECT {PURGE_REQUEST_STATUS_COLUMNS} \
               FROM jobs_runner_purge_requests WHERE request_id = $1"
        );
        tx.query_opt(&select, &[&request_id])?
            .map(purge_request_status_from_pg_row)
            .ok_or(RunnerVolumePurgeError::NotFound)?
    };
    if let Some(account_id) = unlocked_request.account_id.as_deref() {
        lock_account_postgres(&mut tx, account_id)?;
    }
    let current = load_postgres_purge_status_tx(&mut tx, request_id)?;
    if !purge_request_matches_ready_legacy_inventory(
        &current,
        &fleet_legacy_state,
        fleet_legacy_generation,
        fleet_legacy_reconciliation_id.as_deref(),
        fleet_legacy_authority_id.as_deref(),
        fleet_legacy_authority_sha256.as_deref(),
    ) {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    if current.state == "complete" {
        let tombstone_generation = exact_postgres_completed_tombstone(&mut tx, &current)?;
        tx.commit()?;
        return Ok(RunnerPurgeCompletion {
            status: current,
            tombstone_generation,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    let volume_ids = tx
        .query(
            "SELECT volume_id, current_epoch FROM jobs_runner_volumes \
              WHERE status <> 'destroyed' ORDER BY volume_id, current_epoch FOR UPDATE",
            &[],
        )?
        .into_iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, i64>(1)))
        .collect::<Vec<_>>();
    let status = recompute_postgres_purge_request(&mut tx, request_id, completed_at_ms)?;
    if status.legacy_unresolved_count != 0
        || status.resolved_target_count != status.required_target_count
    {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let tombstone_generation = fleet_tombstone_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_purge_tombstones ( \
            purge_subject, request_id, purge_generation, tombstone_generation, \
            target_set_sha256, required_target_count, completed_at_ms \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &status.purge_subject,
            &status.request_id,
            &status.purge_generation,
            &tombstone_generation,
            &status.target_set_sha256,
            &status.required_target_count,
            &completed_at_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_runner_purge_requests \
            SET state = 'complete', updated_at_ms = $2, completed_at_ms = $2 \
          WHERE request_id = $1 AND state = 'pending'",
        &[&request_id, &completed_at_ms],
    )?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, completed_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET tombstone_generation = $1, updated_at_ms = $2 \
          WHERE singleton_id = 1 AND tombstone_generation = $3",
        &[
            &tombstone_generation,
            &completed_at_ms,
            &fleet_tombstone_generation,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    for (volume_id, enrollment_epoch) in volume_ids {
        tx.execute(
            "UPDATE jobs_runner_volumes \
                SET required_tombstone_generation = $2, \
                    status = CASE WHEN status = 'active' THEN 'reconciling' ELSE status END, \
                    updated_at_ms = $3 \
              WHERE volume_id = $1 AND status <> 'destroyed'",
            &[&volume_id, &tombstone_generation, &completed_at_ms],
        )?;
        reconcile_postgres_volume_cursor(&mut tx, &volume_id, enrollment_epoch, completed_at_ms)?;
    }
    let completed = load_postgres_purge_status_tx(&mut tx, request_id)?;
    tx.commit()?;
    Ok(RunnerPurgeCompletion {
        status: completed,
        tombstone_generation,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn exact_postgres_completed_tombstone(
    tx: &mut postgres::Transaction<'_>,
    status: &RunnerPurgeRequestStatus,
) -> RunnerVolumePurgeResult<i64> {
    let row = tx
        .query_opt(
            "SELECT purge_subject, purge_generation, tombstone_generation, \
                    target_set_sha256, required_target_count, completed_at_ms \
               FROM jobs_runner_purge_tombstones WHERE request_id = $1 FOR UPDATE",
            &[&status.request_id],
        )?
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    if row.get::<_, String>(0) != status.purge_subject
        || row.get::<_, i64>(1) != status.purge_generation
        || row.get::<_, String>(3) != status.target_set_sha256
        || row.get::<_, i64>(4) != status.required_target_count
        || Some(row.get::<_, i64>(5)) != status.completed_at_ms
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(row.get(2))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordRunnerVolumeDestructionRequest {
    pub destruction_id: String,
    pub expected_enrollment_generation: i64,
    pub volume_id: String,
    pub volume_epoch: i64,
    pub volume_key_fingerprint: String,
    pub provider: String,
    pub provider_resource_id: String,
    pub resource_fingerprint: String,
    pub evidence_type: String,
    pub snapshot_inventory_sha256: String,
    pub evidence_sha256: String,
    pub authorization_ref: String,
    pub authorized_by: String,
    pub occurred_at_ms: i64,
    pub recorded_at_ms: i64,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeDestructionRecord {
    pub destruction_id: String,
    pub volume_id: String,
    pub volume_epoch: i64,
    pub volume_key_fingerprint: String,
    pub evidence_sha256: String,
    pub recorded_at_ms: i64,
    pub disposition: RunnerVolumeWriteDisposition,
}

fn validate_runner_volume_destruction(
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<String> {
    require_runner_identifier(&input.destruction_id)?;
    require_base64url(&input.volume_id, 32)?;
    require_sha256(&input.volume_key_fingerprint)?;
    require_runner_identifier(&input.provider)?;
    require_nonempty_text(&input.provider_resource_id, 512)?;
    require_sha256(&input.resource_fingerprint)?;
    if !matches!(
        input.evidence_type.as_str(),
        "provider_volume_destroyed" | "physical_device_destroyed"
    ) {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    require_sha256(&input.snapshot_inventory_sha256)?;
    require_sha256(&input.evidence_sha256)?;
    require_nonempty_text(&input.authorization_ref, 1_024)?;
    require_nonempty_text(&input.authorized_by, 240)?;
    if input.expected_enrollment_generation < 0
        || input.volume_epoch <= 0
        || input.occurred_at_ms < 0
        || input.recorded_at_ms < input.occurred_at_ms
        || !input.details.is_object()
    {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    serde_json::to_string(&input.details)
        .map_err(anyhow::Error::from)
        .map_err(Into::into)
}

pub fn record_runner_volume_destruction(
    pool: &DbPool,
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeDestructionRecord> {
    let details_json = validate_runner_volume_destruction(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_runner_volume_destruction_sqlite(pool, input, &details_json),
        DbPool::Postgres(_) => {
            record_runner_volume_destruction_postgres(pool, input, &details_json)
        }
    })
}

#[derive(Debug)]
struct StoredDestructionEvidence {
    destruction_id: String,
    volume_id: String,
    volume_epoch: i64,
    volume_key_fingerprint: String,
    provider: String,
    provider_resource_id: String,
    resource_fingerprint: String,
    evidence_type: String,
    snapshot_inventory_sha256: String,
    evidence_sha256: String,
    authorization_ref: String,
    authorized_by: String,
    occurred_at_ms: i64,
    recorded_at_ms: i64,
    details_json: String,
}

#[derive(Debug)]
struct DestructionVolumeState {
    provider: String,
    provider_resource_id: String,
    resource_fingerprint: String,
    current_epoch: i64,
    status: String,
    active_instance_id: Option<String>,
    instance_lease_expires_at_ms: Option<i64>,
    last_seen_at_ms: i64,
    key_fingerprint: String,
}

fn validate_destruction_volume_identity(
    state: &DestructionVolumeState,
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<()> {
    if state.provider != input.provider
        || state.provider_resource_id != input.provider_resource_id
        || state.resource_fingerprint != input.resource_fingerprint
        || state.current_epoch != input.volume_epoch
        || state.key_fingerprint != input.volume_key_fingerprint
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn validate_new_destruction_volume_state(
    state: &DestructionVolumeState,
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<()> {
    let instance_is_quiescent = match (
        state.active_instance_id.as_ref(),
        state.instance_lease_expires_at_ms,
    ) {
        (None, None) => true,
        (Some(_), Some(expires_at_ms)) => expires_at_ms <= input.recorded_at_ms,
        _ => false,
    };
    if state.status == "destroyed"
        || state.last_seen_at_ms > input.occurred_at_ms
        || !instance_is_quiescent
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn exact_destruction_evidence(
    stored: &StoredDestructionEvidence,
    input: &RecordRunnerVolumeDestructionRequest,
    details_json: &str,
) -> bool {
    stored.volume_id == input.volume_id
        && stored.volume_epoch == input.volume_epoch
        && stored.volume_key_fingerprint == input.volume_key_fingerprint
        && stored.provider == input.provider
        && stored.provider_resource_id == input.provider_resource_id
        && stored.resource_fingerprint == input.resource_fingerprint
        && stored.evidence_type == input.evidence_type
        && stored.snapshot_inventory_sha256 == input.snapshot_inventory_sha256
        && stored.evidence_sha256 == input.evidence_sha256
        && stored.authorization_ref == input.authorization_ref
        && stored.authorized_by == input.authorized_by
        && stored.occurred_at_ms == input.occurred_at_ms
        && stored.details_json == details_json
}

fn destruction_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredDestructionEvidence> {
    Ok(StoredDestructionEvidence {
        destruction_id: row.get(0)?,
        volume_id: row.get(1)?,
        volume_epoch: row.get(2)?,
        volume_key_fingerprint: row.get(3)?,
        provider: row.get(4)?,
        provider_resource_id: row.get(5)?,
        resource_fingerprint: row.get(6)?,
        evidence_type: row.get(7)?,
        snapshot_inventory_sha256: row.get(8)?,
        evidence_sha256: row.get(9)?,
        authorization_ref: row.get(10)?,
        authorized_by: row.get(11)?,
        occurred_at_ms: row.get(12)?,
        recorded_at_ms: row.get(13)?,
        details_json: row.get(14)?,
    })
}

fn destruction_from_pg_row(row: postgres::Row) -> StoredDestructionEvidence {
    StoredDestructionEvidence {
        destruction_id: row.get(0),
        volume_id: row.get(1),
        volume_epoch: row.get(2),
        volume_key_fingerprint: row.get(3),
        provider: row.get(4),
        provider_resource_id: row.get(5),
        resource_fingerprint: row.get(6),
        evidence_type: row.get(7),
        snapshot_inventory_sha256: row.get(8),
        evidence_sha256: row.get(9),
        authorization_ref: row.get(10),
        authorized_by: row.get(11),
        occurred_at_ms: row.get(12),
        recorded_at_ms: row.get(13),
        details_json: row.get(14),
    }
}

const DESTRUCTION_COLUMNS: &str =
    "destruction_id, volume_id, volume_epoch, volume_key_fingerprint, provider, \
     provider_resource_id, resource_fingerprint, evidence_type, snapshot_inventory_sha256, \
     evidence_sha256, authorization_ref, authorized_by, occurred_at_ms, recorded_at_ms, \
     details_json";

fn record_runner_volume_destruction_sqlite(
    pool: &DbPool,
    input: &RecordRunnerVolumeDestructionRequest,
    details_json: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeDestructionRecord> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (enrollment_generation, destruction_generation): (i64, i64) = tx.query_row(
        "SELECT enrollment_generation, destruction_generation \
           FROM jobs_runner_volume_fleet_state WHERE singleton_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let volume = sqlite_destruction_volume_state(&tx, input)?;
    validate_destruction_volume_identity(&volume, input)?;
    let select = format!(
        "SELECT {DESTRUCTION_COLUMNS} FROM jobs_runner_volume_destructions \
          WHERE volume_id = ?1 AND volume_epoch = ?2"
    );
    if let Some(stored) = tx
        .query_row(
            &select,
            params![input.volume_id, input.volume_epoch],
            destruction_from_sqlite_row,
        )
        .optional()?
    {
        if !exact_destruction_evidence(&stored, input, details_json) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(RunnerVolumeDestructionRecord {
            destruction_id: stored.destruction_id,
            volume_id: stored.volume_id,
            volume_epoch: stored.volume_epoch,
            volume_key_fingerprint: stored.volume_key_fingerprint,
            evidence_sha256: stored.evidence_sha256,
            recorded_at_ms: stored.recorded_at_ms,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    validate_new_destruction_volume_state(&volume, input)?;
    if enrollment_generation != input.expected_enrollment_generation {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let next_destruction_generation = destruction_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    insert_sqlite_destruction(&tx, input, details_json)?;
    let request_ids = sqlite_pending_target_request_ids(&tx, input)?;
    tx.execute(
        "UPDATE jobs_runner_purge_targets \
            SET state = 'destroyed', destruction_id = ?3, updated_at_ms = ?4 \
          WHERE volume_id = ?1 AND volume_epoch = ?2 AND state = 'pending'",
        params![
            input.volume_id,
            input.volume_epoch,
            input.destruction_id,
            input.recorded_at_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_runner_volumes \
            SET status = 'destroyed', active_instance_id = NULL, \
                instance_lease_expires_at_ms = NULL, updated_at_ms = ?2 \
          WHERE volume_id = ?1 AND current_epoch = ?3",
        params![input.volume_id, input.recorded_at_ms, input.volume_epoch],
    )?;
    for request_id in request_ids {
        recompute_sqlite_purge_request(&tx, &request_id, input.recorded_at_ms)?;
    }
    let (storage_attestation_count, storage_attestation_set_sha256) =
        sqlite_runner_storage_attestation_set(&tx)?;
    invalidate_sqlite_runner_fleet_cutover(&tx, input.recorded_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET destruction_generation = ?1, storage_attestation_count = ?4, \
                storage_attestation_set_sha256 = ?5, updated_at_ms = ?2 \
          WHERE singleton_id = 1 AND destruction_generation = ?3",
        params![
            next_destruction_generation,
            input.recorded_at_ms,
            destruction_generation,
            storage_attestation_count,
            storage_attestation_set_sha256,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeDestructionRecord {
        destruction_id: input.destruction_id.clone(),
        volume_id: input.volume_id.clone(),
        volume_epoch: input.volume_epoch,
        volume_key_fingerprint: input.volume_key_fingerprint.clone(),
        evidence_sha256: input.evidence_sha256.clone(),
        recorded_at_ms: input.recorded_at_ms,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

fn sqlite_destruction_volume_state(
    tx: &RunnerSqliteTransaction<'_>,
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<DestructionVolumeState> {
    tx.query_row(
        "SELECT v.provider, v.provider_resource_id, v.resource_fingerprint, \
                    v.current_epoch, v.status, v.active_instance_id, \
                    v.instance_lease_expires_at_ms, v.last_seen_at_ms, k.key_fingerprint \
               FROM jobs_runner_volumes v JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = ?2 \
              WHERE v.volume_id = ?1",
        params![input.volume_id, input.volume_epoch],
        |row| {
            Ok(DestructionVolumeState {
                provider: row.get(0)?,
                provider_resource_id: row.get(1)?,
                resource_fingerprint: row.get(2)?,
                current_epoch: row.get(3)?,
                status: row.get(4)?,
                active_instance_id: row.get(5)?,
                instance_lease_expires_at_ms: row.get(6)?,
                last_seen_at_ms: row.get(7)?,
                key_fingerprint: row.get(8)?,
            })
        },
    )
    .optional()?
    .ok_or(RunnerVolumePurgeError::NotFound)
}

fn insert_sqlite_destruction(
    tx: &RunnerSqliteTransaction<'_>,
    input: &RecordRunnerVolumeDestructionRequest,
    details_json: &str,
) -> RunnerVolumePurgeResult<()> {
    tx.execute(
        "INSERT INTO jobs_runner_volume_destructions ( \
            destruction_id, volume_id, volume_epoch, volume_key_fingerprint, provider, \
            provider_resource_id, resource_fingerprint, evidence_type, \
            snapshot_inventory_sha256, evidence_sha256, authorization_ref, authorized_by, \
            occurred_at_ms, recorded_at_ms, details_json \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            input.destruction_id,
            input.volume_id,
            input.volume_epoch,
            input.volume_key_fingerprint,
            input.provider,
            input.provider_resource_id,
            input.resource_fingerprint,
            input.evidence_type,
            input.snapshot_inventory_sha256,
            input.evidence_sha256,
            input.authorization_ref,
            input.authorized_by,
            input.occurred_at_ms,
            input.recorded_at_ms,
            details_json,
        ],
    )?;
    Ok(())
}

fn sqlite_pending_target_request_ids(
    tx: &RunnerSqliteTransaction<'_>,
    input: &RecordRunnerVolumeDestructionRequest,
) -> RunnerVolumePurgeResult<Vec<String>> {
    let mut statement = tx.prepare(
        "SELECT request_id FROM jobs_runner_purge_targets \
          WHERE volume_id = ?1 AND volume_epoch = ?2 AND state = 'pending' \
          ORDER BY request_id",
    )?;
    let rows = statement.query_map(params![input.volume_id, input.volume_epoch], |row| {
        row.get(0)
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn record_runner_volume_destruction_postgres(
    pool: &DbPool,
    input: &RecordRunnerVolumeDestructionRequest,
    details_json: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeDestructionRecord> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let fleet = tx.query_one(
        "SELECT enrollment_generation, destruction_generation \
           FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let enrollment_generation: i64 = fleet.get(0);
    let destruction_generation: i64 = fleet.get(1);
    let volume_row = tx
        .query_opt(
            "SELECT v.provider, v.provider_resource_id, v.resource_fingerprint, \
                    v.current_epoch, v.status, v.active_instance_id, \
                    v.instance_lease_expires_at_ms, v.last_seen_at_ms, k.key_fingerprint \
               FROM jobs_runner_volumes v JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = $2 \
              WHERE v.volume_id = $1 FOR UPDATE OF v, k",
            &[&input.volume_id, &input.volume_epoch],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let volume = DestructionVolumeState {
        provider: volume_row.get(0),
        provider_resource_id: volume_row.get(1),
        resource_fingerprint: volume_row.get(2),
        current_epoch: volume_row.get(3),
        status: volume_row.get(4),
        active_instance_id: volume_row.get(5),
        instance_lease_expires_at_ms: volume_row.get(6),
        last_seen_at_ms: volume_row.get(7),
        key_fingerprint: volume_row.get(8),
    };
    validate_destruction_volume_identity(&volume, input)?;
    let select = format!(
        "SELECT {DESTRUCTION_COLUMNS} FROM jobs_runner_volume_destructions \
          WHERE volume_id = $1 AND volume_epoch = $2 FOR UPDATE"
    );
    if let Some(row) = tx.query_opt(&select, &[&input.volume_id, &input.volume_epoch])? {
        let stored = destruction_from_pg_row(row);
        if !exact_destruction_evidence(&stored, input, details_json) {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        tx.commit()?;
        return Ok(RunnerVolumeDestructionRecord {
            destruction_id: stored.destruction_id,
            volume_id: stored.volume_id,
            volume_epoch: stored.volume_epoch,
            volume_key_fingerprint: stored.volume_key_fingerprint,
            evidence_sha256: stored.evidence_sha256,
            recorded_at_ms: stored.recorded_at_ms,
            disposition: RunnerVolumeWriteDisposition::Replay,
        });
    }
    validate_new_destruction_volume_state(&volume, input)?;
    if enrollment_generation != input.expected_enrollment_generation {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let next_destruction_generation = destruction_generation
        .checked_add(1)
        .ok_or(RunnerVolumePurgeError::Conflict)?;
    tx.execute(
        "INSERT INTO jobs_runner_volume_destructions ( \
            destruction_id, volume_id, volume_epoch, volume_key_fingerprint, provider, \
            provider_resource_id, resource_fingerprint, evidence_type, \
            snapshot_inventory_sha256, evidence_sha256, authorization_ref, authorized_by, \
            occurred_at_ms, recorded_at_ms, details_json \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        &[
            &input.destruction_id,
            &input.volume_id,
            &input.volume_epoch,
            &input.volume_key_fingerprint,
            &input.provider,
            &input.provider_resource_id,
            &input.resource_fingerprint,
            &input.evidence_type,
            &input.snapshot_inventory_sha256,
            &input.evidence_sha256,
            &input.authorization_ref,
            &input.authorized_by,
            &input.occurred_at_ms,
            &input.recorded_at_ms,
            &details_json,
        ],
    )?;
    let request_ids = tx
        .query(
            "SELECT request_id FROM jobs_runner_purge_targets \
              WHERE volume_id = $1 AND volume_epoch = $2 AND state = 'pending' \
              ORDER BY request_id FOR UPDATE",
            &[&input.volume_id, &input.volume_epoch],
        )?
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    tx.execute(
        "UPDATE jobs_runner_purge_targets \
            SET state = 'destroyed', destruction_id = $3, updated_at_ms = $4 \
          WHERE volume_id = $1 AND volume_epoch = $2 AND state = 'pending'",
        &[
            &input.volume_id,
            &input.volume_epoch,
            &input.destruction_id,
            &input.recorded_at_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_runner_volumes \
            SET status = 'destroyed', active_instance_id = NULL, \
                instance_lease_expires_at_ms = NULL, updated_at_ms = $2 \
          WHERE volume_id = $1 AND current_epoch = $3",
        &[&input.volume_id, &input.recorded_at_ms, &input.volume_epoch],
    )?;
    for request_id in request_ids {
        recompute_postgres_purge_request(&mut tx, &request_id, input.recorded_at_ms)?;
    }
    let (storage_attestation_count, storage_attestation_set_sha256) =
        postgres_runner_storage_attestation_set(&mut tx)?;
    invalidate_postgres_runner_fleet_cutover(&mut tx, input.recorded_at_ms)?;
    if tx.execute(
        "UPDATE jobs_runner_volume_fleet_state \
            SET destruction_generation = $1, storage_attestation_count = $4, \
                storage_attestation_set_sha256 = $5, updated_at_ms = $2 \
          WHERE singleton_id = 1 AND destruction_generation = $3",
        &[
            &next_destruction_generation,
            &input.recorded_at_ms,
            &destruction_generation,
            &storage_attestation_count,
            &storage_attestation_set_sha256,
        ],
    )? != 1
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.commit()?;
    Ok(RunnerVolumeDestructionRecord {
        destruction_id: input.destruction_id.clone(),
        volume_id: input.volume_id.clone(),
        volume_epoch: input.volume_epoch,
        volume_key_fingerprint: input.volume_key_fingerprint.clone(),
        evidence_sha256: input.evidence_sha256.clone(),
        recorded_at_ms: input.recorded_at_ms,
        disposition: RunnerVolumeWriteDisposition::Applied,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindRunnerVolumeResidencyRequest {
    pub account_id: String,
    pub run_id: String,
    pub worker_id: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeResidencyBinding {
    pub account_id: String,
    pub run_id: String,
    pub purge_subject: String,
    pub purge_subject_sha256: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub volume_key_fingerprint: String,
    pub process_instance_id: String,
    pub disposition: RunnerVolumeWriteDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedRunnerVolumeLeaseBinding {
    account_id: String,
    run_id: String,
    worker_id: String,
    purge_subject: String,
    purge_subject_sha256: String,
    volume_id: String,
    enrollment_epoch: i64,
    volume_key_fingerprint: String,
    process_instance_id: String,
    now_ms: i64,
}

fn validate_bind_runner_volume_residency(
    input: &BindRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<()> {
    require_nonempty_text(&input.account_id, 240)?;
    require_nonempty_text(&input.run_id, 240)?;
    require_runner_identifier(&input.worker_id)?;
    require_base64url(&input.volume_id, 32)?;
    require_base64url(&input.process_instance_id, 32)?;
    if input.enrollment_epoch <= 0 || input.now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

pub fn bind_runner_volume_residency_and_lease(
    pool: &DbPool,
    input: &BindRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyBinding> {
    validate_bind_runner_volume_residency(input)?;
    let proposed_subject = new_runner_volume_purge_subject();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => bind_runner_volume_residency_sqlite(pool, input, &proposed_subject),
        DbPool::Postgres(_) => {
            bind_runner_volume_residency_postgres(pool, input, &proposed_subject)
        }
    })
}

pub(crate) fn new_runner_volume_purge_subject() -> String {
    let mut subject_material = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut subject_material);
    encode_base64url(&subject_material)
}

fn bind_runner_volume_residency_sqlite(
    pool: &DbPool,
    input: &BindRunnerVolumeResidencyRequest,
    proposed_subject: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyBinding> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let prepared = prepare_runner_volume_lease_binding_sqlite_tx(&tx, input, proposed_subject)?;
    let binding = finalize_runner_volume_lease_binding_sqlite_tx(&tx, input, &prepared)?;
    tx.commit()?;
    Ok(binding)
}

/// Locks and validates runner-volume authority before the execution lease row
/// is claimed. The caller must retain this transaction and pass the returned
/// value to `finalize_runner_volume_lease_binding_sqlite_tx` after inserting or
/// refreshing the prepared lease.
pub(crate) fn prepare_runner_volume_lease_binding_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
    proposed_subject: &str,
) -> RunnerVolumePurgeResult<PreparedRunnerVolumeLeaseBinding> {
    validate_bind_runner_volume_residency(input)?;
    require_base64url(proposed_subject, 32)?;
    let fleet_ready: bool = tx.query_row(
        "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1",
        [],
        |row| row.get(0),
    )?;
    ensure_account_exists_sqlite(tx, &input.account_id)?;
    if !fleet_ready || sqlite_account_has_deletion_fence(tx, &input.account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let subject =
        ensure_sqlite_subject_for_prepare(tx, &input.account_id, proposed_subject, input.now_ms)?;
    let subject_blocked: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests \
          WHERE account_id = ?1 OR purge_subject = ?2) \
         OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones WHERE purge_subject = ?2)",
        params![input.account_id, subject.purge_subject],
        |row| row.get(0),
    )?;
    if subject.legacy_unresolved || subject_blocked != 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let volume_key_fingerprint = validate_sqlite_binding_volume(tx, input)?;
    let purge_subject_sha256 = runner_purge_subject_sha256(&subject.purge_subject)?;
    Ok(PreparedRunnerVolumeLeaseBinding {
        account_id: input.account_id.clone(),
        run_id: input.run_id.clone(),
        worker_id: input.worker_id.clone(),
        purge_subject: subject.purge_subject,
        purge_subject_sha256,
        volume_id: input.volume_id.clone(),
        enrollment_epoch: input.enrollment_epoch,
        volume_key_fingerprint,
        process_instance_id: input.process_instance_id.clone(),
        now_ms: input.now_ms,
    })
}

pub(crate) fn finalize_runner_volume_lease_binding_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
    prepared: &PreparedRunnerVolumeLeaseBinding,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyBinding> {
    require_prepared_lease_binding_matches(prepared, input)?;
    validate_sqlite_binding_lease(tx, input)?;
    let disposition = insert_sqlite_residency_and_binding(
        tx,
        input,
        &prepared.purge_subject,
        &prepared.purge_subject_sha256,
    )?;
    Ok(residency_binding_from_prepared(prepared, disposition))
}

fn validate_sqlite_binding_volume(
    tx: &RunnerSqliteTransaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<String> {
    tx.query_row(
        "SELECT k.key_fingerprint FROM jobs_runner_volumes v \
            JOIN jobs_runner_volume_keys k \
              ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
             WHERE v.volume_id = ?1 AND v.current_epoch = ?2 AND v.status = 'active' \
               AND v.worker_id = ?5 AND k.retired_at_ms IS NULL \
               AND v.active_instance_id = ?3 AND v.instance_lease_expires_at_ms > ?4 \
               AND required_tombstone_generation = reconciled_tombstone_generation \
               AND EXISTS (SELECT 1 FROM jobs_runner_volume_storage_attestations a \
                 WHERE a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                   AND a.volume_key_fingerprint = k.key_fingerprint \
                   AND a.process_instance_id = v.active_instance_id \
                   AND a.enrollment_generation = v.enrollment_generation \
                   AND a.required_tombstone_generation = v.required_tombstone_generation \
                   AND a.reconciled_tombstone_generation = \
                       v.reconciled_tombstone_generation \
                   AND NOT EXISTS (SELECT 1 \
                     FROM jobs_runner_volume_storage_attestations newer \
                    WHERE newer.volume_id = a.volume_id \
                      AND newer.enrollment_epoch = a.enrollment_epoch \
                      AND newer.attestation_generation > a.attestation_generation))",
        params![
            input.volume_id,
            input.enrollment_epoch,
            input.process_instance_id,
            input.now_ms,
            input.worker_id,
        ],
        |row| row.get(0),
    )
    .optional()?
    .ok_or(RunnerVolumePurgeError::NotReady)
}

fn validate_sqlite_binding_lease(
    tx: &RunnerSqliteTransaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<()> {
    let valid: i64 = tx.query_row(
        "SELECT EXISTS( \
            SELECT 1 FROM jobs_execution_leases \
             WHERE run_id = ?1 AND account_id = ?2 AND phase = 'prepared' \
               AND lease_expires_at_ms > ?3 \
        )",
        params![input.run_id, input.account_id, input.now_ms],
        |row| row.get(0),
    )?;
    if valid == 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

fn insert_sqlite_residency_and_binding(
    tx: &RunnerSqliteTransaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
    purge_subject: &str,
    purge_subject_sha256: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    let residency = tx
        .query_row(
            "SELECT state, purge_generation FROM jobs_runner_volume_residencies \
              WHERE purge_subject = ?1 AND volume_id = ?2 AND volume_epoch = ?3",
            params![purge_subject, input.volume_id, input.enrollment_epoch],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    if residency
        .as_ref()
        .is_some_and(|(state, generation)| state != "resident" || *generation != 0)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let binding = tx
        .query_row(
            "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256 \
               FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1",
            params![input.run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    if let Some(binding) = binding {
        if binding.0 != input.volume_id
            || binding.1 != input.enrollment_epoch
            || binding.3 != purge_subject_sha256
            || residency.is_none()
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        if binding.2 == input.process_instance_id {
            return Ok(RunnerVolumeWriteDisposition::Replay);
        }
        if tx.execute(
            "UPDATE jobs_execution_lease_volume_bindings \
                SET process_instance_id = ?2, bound_at_ms = ?3 \
              WHERE run_id = ?1 AND process_instance_id = ?4",
            params![
                input.run_id,
                input.process_instance_id,
                input.now_ms,
                binding.2,
            ],
        )? != 1
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        return Ok(RunnerVolumeWriteDisposition::Applied);
    }
    if residency.is_none() {
        tx.execute(
            "INSERT INTO jobs_runner_volume_residencies ( \
                purge_subject, volume_id, volume_epoch, state, purge_generation, \
                first_recorded_at_ms, last_recorded_at_ms \
             ) VALUES (?1, ?2, ?3, 'resident', 0, ?4, ?4)",
            params![
                purge_subject,
                input.volume_id,
                input.enrollment_epoch,
                input.now_ms
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO jobs_execution_lease_volume_bindings ( \
            run_id, volume_id, volume_epoch, process_instance_id, purge_subject_sha256, \
            bound_at_ms \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            input.run_id,
            input.volume_id,
            input.enrollment_epoch,
            input.process_instance_id,
            purge_subject_sha256,
            input.now_ms,
        ],
    )?;
    Ok(RunnerVolumeWriteDisposition::Applied)
}

fn require_prepared_lease_binding_matches(
    prepared: &PreparedRunnerVolumeLeaseBinding,
    input: &BindRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<()> {
    if prepared.account_id != input.account_id
        || prepared.run_id != input.run_id
        || prepared.worker_id != input.worker_id
        || prepared.volume_id != input.volume_id
        || prepared.enrollment_epoch != input.enrollment_epoch
        || prepared.process_instance_id != input.process_instance_id
        || prepared.now_ms != input.now_ms
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    Ok(())
}

fn residency_binding_from_prepared(
    prepared: &PreparedRunnerVolumeLeaseBinding,
    disposition: RunnerVolumeWriteDisposition,
) -> RunnerVolumeResidencyBinding {
    RunnerVolumeResidencyBinding {
        account_id: prepared.account_id.clone(),
        run_id: prepared.run_id.clone(),
        purge_subject: prepared.purge_subject.clone(),
        purge_subject_sha256: prepared.purge_subject_sha256.clone(),
        volume_id: prepared.volume_id.clone(),
        enrollment_epoch: prepared.enrollment_epoch,
        volume_key_fingerprint: prepared.volume_key_fingerprint.clone(),
        process_instance_id: prepared.process_instance_id.clone(),
        disposition,
    }
}

fn bind_runner_volume_residency_postgres(
    pool: &DbPool,
    input: &BindRunnerVolumeResidencyRequest,
    proposed_subject: &str,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyBinding> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let prepared =
        prepare_runner_volume_lease_binding_postgres_tx(&mut tx, input, proposed_subject)?;
    let binding = finalize_runner_volume_lease_binding_postgres_tx(&mut tx, input, &prepared)?;
    tx.commit()?;
    Ok(binding)
}

/// PostgreSQL half of the two-stage atomic lease binding. Call this before
/// claiming the execution lease so the shared lock order remains fleet,
/// account, volume/key, then lease.
pub(crate) fn prepare_runner_volume_lease_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
    proposed_subject: &str,
) -> RunnerVolumePurgeResult<PreparedRunnerVolumeLeaseBinding> {
    validate_bind_runner_volume_residency(input)?;
    require_base64url(proposed_subject, 32)?;
    let fleet_ready: bool = tx
        .query_one(
            "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
              WHERE singleton_id = 1 FOR SHARE",
            &[],
        )?
        .get(0);
    lock_account_postgres(tx, &input.account_id)?;
    if !fleet_ready || postgres_account_has_deletion_fence(tx, &input.account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let subject =
        ensure_postgres_subject_for_prepare(tx, &input.account_id, proposed_subject, input.now_ms)?;
    let subject_blocked: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests \
              WHERE account_id = $1 OR purge_subject = $2) \
             OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones WHERE purge_subject = $2)",
            &[&input.account_id, &subject.purge_subject],
        )?
        .get(0);
    if subject.legacy_unresolved || subject_blocked {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let volume = tx
        .query_opt(
            "SELECT v.current_epoch = $2 AND v.status = 'active' AND v.worker_id = $5 \
                    AND v.active_instance_id = $3 AND v.instance_lease_expires_at_ms > $4 \
                    AND k.retired_at_ms IS NULL \
                    AND v.required_tombstone_generation = v.reconciled_tombstone_generation \
                    AND EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations a \
                     WHERE a.volume_id = v.volume_id \
                       AND a.enrollment_epoch = v.current_epoch \
                       AND a.volume_key_fingerprint = k.key_fingerprint \
                       AND a.process_instance_id = v.active_instance_id \
                       AND a.enrollment_generation = v.enrollment_generation \
                       AND a.required_tombstone_generation = \
                           v.required_tombstone_generation \
                       AND a.reconciled_tombstone_generation = \
                           v.reconciled_tombstone_generation \
                       AND NOT EXISTS (SELECT 1 \
                         FROM jobs_runner_volume_storage_attestations newer \
                        WHERE newer.volume_id = a.volume_id \
                          AND newer.enrollment_epoch = a.enrollment_epoch \
                          AND newer.attestation_generation > a.attestation_generation)), \
                    k.key_fingerprint \
               FROM jobs_runner_volumes v JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = $1 FOR UPDATE OF v, k",
            &[
                &input.volume_id,
                &input.enrollment_epoch,
                &input.process_instance_id,
                &input.now_ms,
                &input.worker_id,
            ],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    if !volume.get::<_, bool>(0) {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let volume_key_fingerprint: String = volume.get(1);
    let purge_subject_sha256 = runner_purge_subject_sha256(&subject.purge_subject)?;
    tx.query_opt(
        "SELECT state, purge_generation FROM jobs_runner_volume_residencies
          WHERE purge_subject = $1 AND volume_id = $2 AND volume_epoch = $3 FOR UPDATE",
        &[
            &subject.purge_subject,
            &input.volume_id,
            &input.enrollment_epoch,
        ],
    )?;
    tx.query_opt(
        "SELECT volume_id FROM jobs_execution_lease_volume_bindings
          WHERE run_id = $1 FOR UPDATE",
        &[&input.run_id],
    )?;
    Ok(PreparedRunnerVolumeLeaseBinding {
        account_id: input.account_id.clone(),
        run_id: input.run_id.clone(),
        worker_id: input.worker_id.clone(),
        purge_subject: subject.purge_subject,
        purge_subject_sha256,
        volume_id: input.volume_id.clone(),
        enrollment_epoch: input.enrollment_epoch,
        volume_key_fingerprint,
        process_instance_id: input.process_instance_id.clone(),
        now_ms: input.now_ms,
    })
}

pub(crate) fn finalize_runner_volume_lease_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &BindRunnerVolumeResidencyRequest,
    prepared: &PreparedRunnerVolumeLeaseBinding,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyBinding> {
    require_prepared_lease_binding_matches(prepared, input)?;
    tx.execute(
        "INSERT INTO jobs_runner_account_subjects (
            account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms
         ) VALUES ($1, $2, FALSE, $3, $3)
         ON CONFLICT(account_id) DO NOTHING",
        &[&input.account_id, &prepared.purge_subject, &input.now_ms],
    )?;
    let stored_subject = runner_account_subject_from_pg_row(tx.query_one(
        "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms
           FROM jobs_runner_account_subjects WHERE account_id = $1 FOR UPDATE",
        &[&input.account_id],
    )?);
    if stored_subject.purge_subject != prepared.purge_subject || stored_subject.legacy_unresolved {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let lease_valid: bool = tx
        .query_opt(
            "SELECT account_id = $2 AND phase = 'prepared' AND lease_expires_at_ms > $3 \
               FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
            &[&input.run_id, &input.account_id, &input.now_ms],
        )?
        .ok_or(RunnerVolumePurgeError::NotFound)?
        .get(0);
    if !lease_valid {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let residency = tx.query_opt(
        "SELECT state, purge_generation FROM jobs_runner_volume_residencies \
          WHERE purge_subject = $1 AND volume_id = $2 AND volume_epoch = $3 FOR UPDATE",
        &[
            &prepared.purge_subject,
            &input.volume_id,
            &input.enrollment_epoch,
        ],
    )?;
    if residency
        .as_ref()
        .is_some_and(|row| row.get::<_, String>(0) != "resident" || row.get::<_, i64>(1) != 0)
    {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let binding = tx.query_opt(
        "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256 \
           FROM jobs_execution_lease_volume_bindings WHERE run_id = $1 FOR UPDATE",
        &[&input.run_id],
    )?;
    let disposition = if let Some(binding) = binding {
        let bound_process_instance_id: String = binding.get(2);
        if binding.get::<_, String>(0) != input.volume_id
            || binding.get::<_, i64>(1) != input.enrollment_epoch
            || binding.get::<_, String>(3) != prepared.purge_subject_sha256
            || residency.is_none()
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        if bound_process_instance_id == input.process_instance_id {
            RunnerVolumeWriteDisposition::Replay
        } else {
            if tx.execute(
                "UPDATE jobs_execution_lease_volume_bindings \
                    SET process_instance_id = $2, bound_at_ms = $3 \
                  WHERE run_id = $1 AND process_instance_id = $4",
                &[
                    &input.run_id,
                    &input.process_instance_id,
                    &input.now_ms,
                    &bound_process_instance_id,
                ],
            )? != 1
            {
                return Err(RunnerVolumePurgeError::Conflict);
            }
            RunnerVolumeWriteDisposition::Applied
        }
    } else {
        if residency.is_none() {
            tx.execute(
                "INSERT INTO jobs_runner_volume_residencies ( \
                    purge_subject, volume_id, volume_epoch, state, purge_generation, \
                    first_recorded_at_ms, last_recorded_at_ms \
                 ) VALUES ($1, $2, $3, 'resident', 0, $4, $4)",
                &[
                    &prepared.purge_subject,
                    &input.volume_id,
                    &input.enrollment_epoch,
                    &input.now_ms,
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO jobs_execution_lease_volume_bindings ( \
                run_id, volume_id, volume_epoch, process_instance_id, purge_subject_sha256, \
                bound_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &input.run_id,
                &input.volume_id,
                &input.enrollment_epoch,
                &input.process_instance_id,
                &prepared.purge_subject_sha256,
                &input.now_ms,
            ],
        )?;
        RunnerVolumeWriteDisposition::Applied
    };
    Ok(residency_binding_from_prepared(prepared, disposition))
}

fn validate_current_runner_volume_lease_binding_request(
    account_id: &str,
    run_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    require_nonempty_text(account_id, 240)?;
    require_nonempty_text(run_id, 240)?;
    if now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn require_current_runner_volume_identity_binding_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
    run_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    validate_current_runner_volume_lease_binding_request(account_id, run_id, now_ms)?;
    let fleet_ready: bool = tx.query_row(
        "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
          WHERE singleton_id = 1",
        [],
        |row| row.get(0),
    )?;
    ensure_account_exists_sqlite(tx, account_id)?;
    if !fleet_ready || sqlite_account_has_deletion_fence(tx, account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let subject = tx
        .query_row(
            "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
               FROM jobs_runner_account_subjects WHERE account_id = ?1",
            params![account_id],
            runner_account_subject_from_sqlite_row,
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    let purge_subject_sha256 = runner_purge_subject_sha256(&subject.purge_subject)?;
    let blocked: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests \
          WHERE account_id = ?1 OR purge_subject = ?2) \
         OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones WHERE purge_subject = ?2)",
        params![account_id, subject.purge_subject],
        |row| row.get(0),
    )?;
    if subject.legacy_unresolved || blocked != 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let binding = tx
        .query_row(
            "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256 \
               FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1",
            params![run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    if binding.3 != purge_subject_sha256 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.query_row(
        "SELECT 1 FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = ?1 AND v.current_epoch = ?2 AND v.status = 'active' \
                AND k.retired_at_ms IS NULL AND v.active_instance_id = ?3 \
                AND v.instance_lease_expires_at_ms > ?4 \
                AND v.required_tombstone_generation = v.reconciled_tombstone_generation \
                AND EXISTS (SELECT 1 FROM jobs_runner_volume_storage_attestations a \
                  WHERE a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                    AND a.volume_key_fingerprint = k.key_fingerprint \
                    AND a.process_instance_id = v.active_instance_id \
                    AND a.enrollment_generation = v.enrollment_generation \
                    AND a.required_tombstone_generation = v.required_tombstone_generation \
                    AND a.reconciled_tombstone_generation = \
                        v.reconciled_tombstone_generation \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation))",
        params![binding.0, binding.1, binding.2, now_ms],
        |_| Ok(()),
    )
    .optional()?
    .ok_or(RunnerVolumePurgeError::NotReady)?;
    let residency_valid: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_runner_volume_residencies \
          WHERE purge_subject = ?1 AND volume_id = ?2 AND volume_epoch = ?3 \
            AND state = 'resident' AND purge_generation = 0)",
        params![subject.purge_subject, binding.0, binding.1],
        |row| row.get(0),
    )?;
    if residency_valid == 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

pub(crate) fn require_current_runner_volume_lease_binding_sqlite_tx(
    tx: &RunnerSqliteTransaction<'_>,
    account_id: &str,
    run_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    require_current_runner_volume_identity_binding_sqlite_tx(tx, account_id, run_id, now_ms)?;
    let lease_valid: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_execution_leases \
          WHERE run_id = ?1 AND account_id = ?2 AND phase IN ('prepared', 'click_started') \
            AND lease_expires_at_ms > ?3)",
        params![run_id, account_id, now_ms],
        |row| row.get(0),
    )?;
    if lease_valid == 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

pub(crate) fn require_current_runner_volume_identity_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    validate_current_runner_volume_lease_binding_request(account_id, run_id, now_ms)?;
    let fleet_ready: bool = tx
        .query_one(
            "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
              WHERE singleton_id = 1 FOR SHARE",
            &[],
        )?
        .get(0);
    lock_account_postgres(tx, account_id)?;
    if !fleet_ready || postgres_account_has_deletion_fence(tx, account_id)? {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let subject_row = tx
        .query_opt(
            "SELECT account_id, purge_subject, legacy_unresolved, created_at_ms, updated_at_ms \
               FROM jobs_runner_account_subjects WHERE account_id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    let subject = runner_account_subject_from_pg_row(subject_row);
    let purge_subject_sha256 = runner_purge_subject_sha256(&subject.purge_subject)?;
    let blocked: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests \
              WHERE account_id = $1 OR purge_subject = $2) \
             OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones WHERE purge_subject = $2)",
            &[&account_id, &subject.purge_subject],
        )?
        .get(0);
    if subject.legacy_unresolved || blocked {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    let discovered = tx
        .query_opt(
            "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256 \
               FROM jobs_execution_lease_volume_bindings WHERE run_id = $1",
            &[&run_id],
        )?
        .ok_or(RunnerVolumePurgeError::NotReady)?;
    let discovered_binding: (String, i64, String, String) = (
        discovered.get(0),
        discovered.get(1),
        discovered.get(2),
        discovered.get(3),
    );
    if discovered_binding.3 != purge_subject_sha256 {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    tx.query_opt(
        "SELECT 1 FROM jobs_runner_volumes v \
               JOIN jobs_runner_volume_keys k \
                 ON k.volume_id = v.volume_id AND k.enrollment_epoch = v.current_epoch \
              WHERE v.volume_id = $1 AND v.current_epoch = $2 AND v.status = 'active' \
                AND k.retired_at_ms IS NULL AND v.active_instance_id = $3 \
                AND v.instance_lease_expires_at_ms > $4 \
                AND v.required_tombstone_generation = v.reconciled_tombstone_generation \
                AND EXISTS (SELECT 1 FROM jobs_runner_volume_storage_attestations a \
                  WHERE a.volume_id = v.volume_id AND a.enrollment_epoch = v.current_epoch \
                    AND a.volume_key_fingerprint = k.key_fingerprint \
                    AND a.process_instance_id = v.active_instance_id \
                    AND a.enrollment_generation = v.enrollment_generation \
                    AND a.required_tombstone_generation = v.required_tombstone_generation \
                    AND a.reconciled_tombstone_generation = \
                        v.reconciled_tombstone_generation \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation)) \
              FOR UPDATE OF v, k",
        &[
            &discovered_binding.0,
            &discovered_binding.1,
            &discovered_binding.2,
            &now_ms,
        ],
    )?
    .ok_or(RunnerVolumePurgeError::NotReady)?;
    let locked_binding = tx.query_one(
        "SELECT volume_id, volume_epoch, process_instance_id, purge_subject_sha256 \
               FROM jobs_execution_lease_volume_bindings WHERE run_id = $1 FOR UPDATE",
        &[&run_id],
    )?;
    let locked_binding: (String, i64, String, String) = (
        locked_binding.get(0),
        locked_binding.get(1),
        locked_binding.get(2),
        locked_binding.get(3),
    );
    if locked_binding != discovered_binding {
        return Err(RunnerVolumePurgeError::Conflict);
    }
    let residency_valid: bool = tx
        .query_opt(
            "SELECT state = 'resident' AND purge_generation = 0 \
               FROM jobs_runner_volume_residencies \
              WHERE purge_subject = $1 AND volume_id = $2 AND volume_epoch = $3 FOR UPDATE",
            &[
                &subject.purge_subject,
                &discovered_binding.0,
                &discovered_binding.1,
            ],
        )?
        .ok_or(RunnerVolumePurgeError::NotReady)?
        .get(0);
    if !residency_valid {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

pub(crate) fn require_current_runner_volume_lease_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    run_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<()> {
    require_current_runner_volume_identity_binding_postgres_tx(tx, account_id, run_id, now_ms)?;
    let lease_valid: bool = tx
        .query_opt(
            "SELECT account_id = $2 AND phase IN ('prepared', 'click_started') \
                    AND lease_expires_at_ms > $3 \
               FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
            &[&run_id, &account_id, &now_ms],
        )?
        .ok_or(RunnerVolumePurgeError::NotReady)?
        .get(0);
    if !lease_valid {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordRunnerVolumeResidencyRequest {
    pub purge_subject: String,
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunnerVolumeResidencyRecord {
    pub disposition: RunnerVolumeWriteDisposition,
    pub required_tombstone_generation: i64,
    pub reconciled_tombstone_generation: i64,
}

pub fn record_runner_volume_residency(
    pool: &DbPool,
    input: &RecordRunnerVolumeResidencyRequest,
    authority: &VerifiedRunnerVolumeAuthority,
) -> RunnerVolumePurgeResult<RunnerVolumeResidencyRecord> {
    require_base64url(&input.purge_subject, 32)?;
    require_base64url(&input.volume_id, 32)?;
    require_base64url(&input.process_instance_id, 32)?;
    if input.enrollment_epoch <= 0 || input.now_ms < 0 {
        return Err(RunnerVolumePurgeError::InvalidRequest);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let fleet_ready: bool = tx.query_row(
                "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
                  WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            if !fleet_ready {
                return Err(RunnerVolumePurgeError::NotReady);
            }
            require_live_unfenced_sqlite_subject(&tx, &input.purge_subject)?;
            let (required, reconciled) = require_active_sqlite_volume_instance(
                &tx,
                &input.volume_id,
                input.enrollment_epoch,
                &input.process_instance_id,
                input.now_ms,
            )?;
            consume_runner_volume_authority_sqlite_tx(
                &tx,
                authority,
                "residency_bind",
                &input.volume_id,
                input.enrollment_epoch,
                &input.process_instance_id,
                input.now_ms,
            )?;
            let disposition = insert_sqlite_standalone_residency(&tx, input)?;
            tx.commit()?;
            Ok(RunnerVolumeResidencyRecord {
                disposition,
                required_tombstone_generation: required,
                reconciled_tombstone_generation: reconciled,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let fleet_ready: bool = tx
                .query_one(
                    "SELECT cutover_state = 'ready' FROM jobs_runner_volume_fleet_state \
                      WHERE singleton_id = 1 FOR UPDATE",
                    &[],
                )?
                .get(0);
            if !fleet_ready {
                return Err(RunnerVolumePurgeError::NotReady);
            }
            let account_id = tx
                .query_opt(
                    "SELECT account_id FROM jobs_runner_account_subjects \
                      WHERE purge_subject = $1 FOR UPDATE",
                    &[&input.purge_subject],
                )?
                .map(|row| row.get::<_, String>(0))
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            lock_account_postgres(&mut tx, &account_id)?;
            if postgres_account_has_deletion_fence(&mut tx, &account_id)?
                || tx
                    .query_one(
                        "SELECT EXISTS(SELECT 1 FROM jobs_runner_purge_requests \
                          WHERE purge_subject = $1) \
                         OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones \
                          WHERE purge_subject = $1)",
                        &[&input.purge_subject],
                    )?
                    .get::<_, bool>(0)
            {
                return Err(RunnerVolumePurgeError::NotReady);
            }
            let volume = tx
                .query_opt(
                    "SELECT required_tombstone_generation, reconciled_tombstone_generation, \
                            current_epoch = $2 AND status = 'active' \
                            AND active_instance_id = $3 AND instance_lease_expires_at_ms > $4 \
                            AND required_tombstone_generation = reconciled_tombstone_generation \
                            AND EXISTS (SELECT 1 \
                              FROM jobs_runner_volume_storage_attestations a \
                             WHERE a.volume_id = jobs_runner_volumes.volume_id \
                               AND a.enrollment_epoch = jobs_runner_volumes.current_epoch \
                               AND a.process_instance_id = \
                                   jobs_runner_volumes.active_instance_id \
                               AND a.enrollment_generation = \
                                   jobs_runner_volumes.enrollment_generation \
                               AND a.required_tombstone_generation = \
                                   jobs_runner_volumes.required_tombstone_generation \
                               AND a.reconciled_tombstone_generation = \
                                   jobs_runner_volumes.reconciled_tombstone_generation \
                               AND NOT EXISTS (SELECT 1 \
                                 FROM jobs_runner_volume_storage_attestations newer \
                                WHERE newer.volume_id = a.volume_id \
                                  AND newer.enrollment_epoch = a.enrollment_epoch \
                                  AND newer.attestation_generation > \
                                      a.attestation_generation)) \
                       FROM jobs_runner_volumes WHERE volume_id = $1 FOR UPDATE",
                    &[
                        &input.volume_id,
                        &input.enrollment_epoch,
                        &input.process_instance_id,
                        &input.now_ms,
                    ],
                )?
                .ok_or(RunnerVolumePurgeError::NotFound)?;
            if !volume.get::<_, bool>(2) {
                return Err(RunnerVolumePurgeError::NotReady);
            }
            consume_runner_volume_authority_postgres_tx(
                &mut tx,
                authority,
                "residency_bind",
                &input.volume_id,
                input.enrollment_epoch,
                &input.process_instance_id,
                input.now_ms,
            )?;
            let required: i64 = volume.get(0);
            let reconciled: i64 = volume.get(1);
            let existing = tx.query_opt(
                "SELECT state, purge_generation FROM jobs_runner_volume_residencies \
                  WHERE purge_subject = $1 AND volume_id = $2 AND volume_epoch = $3 \
                  FOR UPDATE",
                &[
                    &input.purge_subject,
                    &input.volume_id,
                    &input.enrollment_epoch,
                ],
            )?;
            if let Some(existing) = existing {
                if existing.get::<_, String>(0) != "resident" || existing.get::<_, i64>(1) != 0 {
                    return Err(RunnerVolumePurgeError::Conflict);
                }
                tx.commit()?;
                return Ok(RunnerVolumeResidencyRecord {
                    disposition: RunnerVolumeWriteDisposition::Replay,
                    required_tombstone_generation: required,
                    reconciled_tombstone_generation: reconciled,
                });
            }
            tx.execute(
                "INSERT INTO jobs_runner_volume_residencies ( \
                    purge_subject, volume_id, volume_epoch, state, purge_generation, \
                    first_recorded_at_ms, last_recorded_at_ms \
                 ) VALUES ($1, $2, $3, 'resident', 0, $4, $4)",
                &[
                    &input.purge_subject,
                    &input.volume_id,
                    &input.enrollment_epoch,
                    &input.now_ms,
                ],
            )?;
            tx.commit()?;
            Ok(RunnerVolumeResidencyRecord {
                disposition: RunnerVolumeWriteDisposition::Applied,
                required_tombstone_generation: required,
                reconciled_tombstone_generation: reconciled,
            })
        }
    })
}

fn require_live_unfenced_sqlite_subject(
    tx: &RunnerSqliteTransaction<'_>,
    purge_subject: &str,
) -> RunnerVolumePurgeResult<()> {
    let account_id = tx
        .query_row(
            "SELECT account_id FROM jobs_runner_account_subjects WHERE purge_subject = ?1",
            params![purge_subject],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    let blocked: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM account_deletion_intents WHERE account_id = ?2) \
             OR EXISTS(SELECT 1 FROM jobs_runner_purge_requests WHERE purge_subject = ?1) \
             OR EXISTS(SELECT 1 FROM jobs_runner_purge_tombstones WHERE purge_subject = ?1)",
        params![purge_subject, account_id],
        |row| row.get(0),
    )?;
    if blocked != 0 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok(())
}

fn require_active_sqlite_volume_instance(
    tx: &RunnerSqliteTransaction<'_>,
    volume_id: &str,
    enrollment_epoch: i64,
    process_instance_id: &str,
    now_ms: i64,
) -> RunnerVolumePurgeResult<(i64, i64)> {
    let row = tx
        .query_row(
            "SELECT required_tombstone_generation, reconciled_tombstone_generation, \
                current_epoch = ?2 AND status = 'active' AND active_instance_id = ?3 \
                AND instance_lease_expires_at_ms > ?4 \
                AND required_tombstone_generation = reconciled_tombstone_generation \
                AND EXISTS (SELECT 1 FROM jobs_runner_volume_storage_attestations a \
                  WHERE a.volume_id = jobs_runner_volumes.volume_id \
                    AND a.enrollment_epoch = jobs_runner_volumes.current_epoch \
                    AND a.process_instance_id = jobs_runner_volumes.active_instance_id \
                    AND a.enrollment_generation = jobs_runner_volumes.enrollment_generation \
                    AND a.required_tombstone_generation = \
                        jobs_runner_volumes.required_tombstone_generation \
                    AND a.reconciled_tombstone_generation = \
                        jobs_runner_volumes.reconciled_tombstone_generation \
                    AND NOT EXISTS (SELECT 1 \
                      FROM jobs_runner_volume_storage_attestations newer \
                     WHERE newer.volume_id = a.volume_id \
                       AND newer.enrollment_epoch = a.enrollment_epoch \
                       AND newer.attestation_generation > a.attestation_generation)) \
           FROM jobs_runner_volumes WHERE volume_id = ?1",
            params![volume_id, enrollment_epoch, process_instance_id, now_ms],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(RunnerVolumePurgeError::NotFound)?;
    if !row.2 {
        return Err(RunnerVolumePurgeError::NotReady);
    }
    Ok((row.0, row.1))
}

fn insert_sqlite_standalone_residency(
    tx: &RunnerSqliteTransaction<'_>,
    input: &RecordRunnerVolumeResidencyRequest,
) -> RunnerVolumePurgeResult<RunnerVolumeWriteDisposition> {
    let existing = tx
        .query_row(
            "SELECT state, purge_generation FROM jobs_runner_volume_residencies \
              WHERE purge_subject = ?1 AND volume_id = ?2 AND volume_epoch = ?3",
            params![input.purge_subject, input.volume_id, input.enrollment_epoch],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    if let Some((state, generation)) = existing {
        if state != "resident" || generation != 0 {
            return Err(RunnerVolumePurgeError::Conflict);
        }
        return Ok(RunnerVolumeWriteDisposition::Replay);
    }
    tx.execute(
        "INSERT INTO jobs_runner_volume_residencies ( \
            purge_subject, volume_id, volume_epoch, state, purge_generation, \
            first_recorded_at_ms, last_recorded_at_ms \
         ) VALUES (?1, ?2, ?3, 'resident', 0, ?4, ?4)",
        params![
            input.purge_subject,
            input.volume_id,
            input.enrollment_epoch,
            input.now_ms,
        ],
    )?;
    Ok(RunnerVolumeWriteDisposition::Applied)
}

#[cfg(test)]
pub(super) mod runner_volume_purge_tests {
    use super::*;
    use super::production_positive_authority_fixture::{
        install_production_positive_job_authorities_for_runner,
        save_production_positive_verified_import,
    };
    use crate::db;
    use std::path::PathBuf;

    const TEST_RUNNER_BUILD: &str = "runner-602";

    pub(super) struct InstalledCertifiedCloudRuntimeFixture {
        pub(super) runtime_target: AtsCertificationRuntimeTarget,
        pub(super) binding_request: BindRunnerVolumeResidencyRequest,
        pub(super) runtime_grant_id: String,
        pub(super) runtime_sha256: String,
        pub(super) verified_authority: VerifiedRunnerVolumeAuthority,
    }

    #[test]
    fn runner_process_runtime_digest_matches_runner_canonical_vector() {
        let runtime = RunnerProcessRuntimeAttestation {
            runner_image_sha256: "1".repeat(64),
            runner_build_id: "runner-602.1".to_string(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: "2".repeat(64),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "chromium-123456".to_string(),
            chromium_executable_sha256: "3".repeat(64),
        };
        assert_eq!(
            runner_process_runtime_sha256(&runtime).expect("hash process runtime"),
            "0a6faf7674e8166a8d54d0aea177f73842012dcbd68c15a80fc5ed2571c75dfd"
        );
    }

    #[test]
    fn runtime_grant_revocation_blocks_unbound_claim_and_cannot_erase_bound_runtime() {
        let database = TestDatabase::new(&[]);
        let runtime = RunnerProcessRuntimeAttestation {
            runner_image_sha256: sha256("runtime-revocation-image"),
            runner_build_id: "runner-604.1".to_string(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: sha256("runtime-revocation-bundle"),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "123456".to_string(),
            chromium_executable_sha256: sha256("runtime-revocation-chromium"),
        };

        let revoked_volume = fixed_volume(&database.pool, 91, "runtime-revoked", 10);
        let revoked_token = encode_base64url(&[92_u8; 32]);
        let revoked_grant_id = "runtime-grant-revoked";
        create_runner_process_runtime_grant(
            &database.pool,
            &NewRunnerProcessRuntimeGrant {
                grant_id: revoked_grant_id.to_string(),
                token: revoked_token.clone(),
                expected_worker_id: "worker-runtime-revoked".to_string(),
                runtime: runtime.clone(),
                authorization_ref: "deployment-runtime-revoked".to_string(),
                created_by: "test-operator".to_string(),
                expires_at_ms: 1_000,
                created_at_ms: 10,
            },
        )
        .expect("create revocable runtime grant");
        let revocation = RevokeRunnerProcessRuntimeGrantRequest {
            grant_id: revoked_grant_id.to_string(),
            reason: "image withdrawn before process claim".to_string(),
            authorization_ref: "incident-runtime-revoked".to_string(),
            revoked_by: "test-operator".to_string(),
            revoked_at_ms: 11,
        };
        let applied = revoke_runner_process_runtime_grant(&database.pool, &revocation)
            .expect("revoke unused runtime grant");
        assert_eq!(applied.disposition, RunnerVolumeWriteDisposition::Applied);
        let replay = revoke_runner_process_runtime_grant(&database.pool, &revocation)
            .expect("replay exact runtime revocation");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);
        let revoked_claim = RunnerProcessRuntimeGrantClaim {
            grant_id: revoked_grant_id.to_string(),
            grant_token: revoked_token,
            runtime: runtime.clone(),
        };
        let revoked_request = RunnerVolumeInstanceLeaseRequest {
            volume_id: &revoked_volume.volume_id,
            enrollment_epoch: 1,
            process_instance_id: &revoked_volume.process_instance_id,
            now_ms: 12,
            lease_expires_at_ms: 500,
        };
        assert!(matches!(
            claim_runner_volume_instance_inner(
                &database.pool,
                &revoked_request,
                Some(&revoked_claim),
                None,
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));

        let bound_volume = fixed_volume(&database.pool, 93, "runtime-bound", 20);
        let bound_token = encode_base64url(&[94_u8; 32]);
        let bound_grant_id = "runtime-grant-bound";
        let bound_grant = create_runner_process_runtime_grant(
            &database.pool,
            &NewRunnerProcessRuntimeGrant {
                grant_id: bound_grant_id.to_string(),
                token: bound_token.clone(),
                expected_worker_id: "worker-runtime-bound".to_string(),
                runtime: runtime.clone(),
                authorization_ref: "deployment-runtime-bound".to_string(),
                created_by: "test-operator".to_string(),
                expires_at_ms: 1_000,
                created_at_ms: 20,
            },
        )
        .expect("create bindable runtime grant");
        let bound_claim = RunnerProcessRuntimeGrantClaim {
            grant_id: bound_grant_id.to_string(),
            grant_token: bound_token,
            runtime,
        };
        let bound_request = RunnerVolumeInstanceLeaseRequest {
            volume_id: &bound_volume.volume_id,
            enrollment_epoch: 1,
            process_instance_id: &bound_volume.process_instance_id,
            now_ms: 21,
            lease_expires_at_ms: 500,
        };
        let bound = claim_runner_volume_instance_inner(
            &database.pool,
            &bound_request,
            Some(&bound_claim),
            None,
        )
        .expect("bind approved runtime to process");
        assert_eq!(bound.runtime_grant_id.as_deref(), Some(bound_grant_id));
        assert_eq!(
            bound.runtime_sha256.as_deref(),
            Some(bound_grant.runtime_sha256.as_str())
        );
        let bound_revocation = RevokeRunnerProcessRuntimeGrantRequest {
            grant_id: bound_grant_id.to_string(),
            reason: "attempt to erase bound runtime".to_string(),
            authorization_ref: "incident-runtime-bound".to_string(),
            revoked_by: "test-operator".to_string(),
            revoked_at_ms: 22,
        };
        assert!(matches!(
            revoke_runner_process_runtime_grant(&database.pool, &bound_revocation),
            Err(RunnerVolumePurgeError::Conflict)
        ));
        let mut conn = database
            .pool
            .get()
            .expect("open trusted runtime transaction");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .expect("begin trusted runtime transaction");
        let trusted = require_runner_process_runtime_sqlite_tx(
            &tx,
            "worker-runtime-bound",
            &bound_volume.volume_id,
            1,
            &bound_volume.process_instance_id,
        )
        .expect("bound runtime remains immutable recovery evidence");
        assert_eq!(trusted.runtime_grant_id, bound_grant_id);
        assert_eq!(trusted.runtime_sha256, bound_grant.runtime_sha256);
        tx.commit().expect("commit trusted runtime verification");
    }

    struct TestDatabase {
        pool: DbPool,
        path: PathBuf,
    }

    impl TestDatabase {
        fn new(account_ids: &[&str]) -> Self {
            let path = PathBuf::from("/tmp").join(format!(
                "bluey-runner-volume-purge-{}-{}.sqlite3",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let pool = db::open_pool(&path).expect("open isolated SQLite database");
            db::run_migrations(&pool).expect("apply runtime migrations");
            let conn = pool.get().expect("open test connection");
            for (index, account_id) in account_ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining) \
                     VALUES (?1, ?2, 'hash', 0)",
                    params![account_id, format!("runner-purge-{index}@example.test")],
                )
                .expect("insert test account");
            }
            drop(conn);
            Self { pool, path }
        }

        fn fence_account(&self, account_id: &str, now_ms: i64) {
            self.pool
                .get()
                .expect("open test connection")
                .execute(
                    "INSERT INTO account_deletion_intents ( \
                        account_id, requested_at_ms, last_checked_at_ms, \
                        fresh_upload_cutoff_ms, fresh_in_flight_puts \
                     ) VALUES (?1, ?2, ?2, ?2, 0)",
                    params![account_id, now_ms],
                )
                .expect("fence test account");
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(format!("{}-wal", self.path.display()));
            let _ = std::fs::remove_file(format!("{}-shm", self.path.display()));
        }
    }

    #[derive(Clone)]
    struct FixedVolume {
        signing_key: Ed25519SigningKey,
        volume_id: String,
        key_fingerprint: String,
        provider_resource_id: String,
        resource_fingerprint: String,
        process_instance_id: String,
    }

    fn sha256(value: impl AsRef<[u8]>) -> String {
        hex::encode(Sha256::digest(value.as_ref()))
    }

    fn empty_raw_inventory() -> RunnerPurgeInventoryEvidence {
        RunnerPurgeInventoryEvidence {
            entry_count: 0,
            file_bytes: "0".to_string(),
            sha256: EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
        }
    }

    fn empty_subject_inventory() -> RunnerPurgeInventoryEvidence {
        RunnerPurgeInventoryEvidence {
            entry_count: 0,
            file_bytes: "0".to_string(),
            sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256.to_string(),
        }
    }

    fn test_storage_evidence(label: &str, legacy_entry_count: i64) -> RunnerPurgeStorageEvidence {
        assert!(legacy_entry_count > 0);
        let artifact_bytes = legacy_entry_count
            .checked_mul(7)
            .expect("bounded legacy fixture bytes")
            .to_string();
        let empty_subject = empty_subject_inventory();
        RunnerPurgeStorageEvidence {
            version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            root: RunnerPurgeRootEvidence {
                device_id: "unix:602:mount:17".to_string(),
                before: RunnerPurgeRootSnapshotEvidence {
                    link_count: 7,
                    entry_count: legacy_entry_count + 2,
                    file_bytes: artifact_bytes.clone(),
                    sha256: sha256(format!("root-before-{label}")),
                },
                after: RunnerPurgeRootSnapshotEvidence {
                    link_count: 7,
                    entry_count: 2,
                    file_bytes: "0".to_string(),
                    sha256: sha256(format!("root-after-{label}")),
                },
            },
            locators: RunnerPurgeLocatorEvidence {
                count: 0,
                sha256: sha256("empty-locators"),
            },
            subject_storage: RunnerPurgeSubjectStorageEvidence {
                layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
                before: RunnerPurgeSubjectStorageSnapshotEvidence {
                    residency: "never_resident".to_string(),
                    subject_tree: empty_subject.clone(),
                    ownership: empty_subject.clone(),
                    target: empty_subject.clone(),
                    complete_root: empty_subject.clone(),
                },
                after: RunnerPurgeSubjectStorageSnapshotEvidence {
                    residency: "never_resident".to_string(),
                    subject_tree: empty_subject.clone(),
                    ownership: empty_subject.clone(),
                    target: empty_subject.clone(),
                    complete_root: empty_subject,
                },
            },
            legacy: RunnerPurgeLegacyEvidence {
                inventory_version: RUNNER_LEGACY_INVENTORY_VERSION,
                target_before: RunnerPurgeInventoryEvidence {
                    entry_count: legacy_entry_count,
                    file_bytes: artifact_bytes.clone(),
                    sha256: sha256(format!("legacy-target-before-{label}")),
                },
                target_after: empty_raw_inventory(),
                root_before: RunnerPurgeLegacyRootSnapshotEvidence {
                    artifact_count: legacy_entry_count,
                    artifact_bytes,
                    artifact_set_sha256: sha256(format!("legacy-set-before-{label}")),
                    unclassified_root_count: 0,
                },
                root_after: RunnerPurgeLegacyRootSnapshotEvidence {
                    artifact_count: 0,
                    artifact_bytes: "0".to_string(),
                    artifact_set_sha256: EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.to_string(),
                    unclassified_root_count: 0,
                },
            },
        }
    }

    fn mixed_storage_evidence(label: &str) -> RunnerPurgeStorageEvidence {
        let mut evidence = test_storage_evidence(label, 2);
        evidence.root.before.entry_count = 10;
        evidence.root.before.file_bytes = "30".to_string();
        evidence.locators.count = 1;
        evidence.locators.sha256 = sha256(format!("locators-{label}"));
        evidence.subject_storage.before = RunnerPurgeSubjectStorageSnapshotEvidence {
            residency: "resident".to_string(),
            subject_tree: RunnerPurgeInventoryEvidence {
                entry_count: 2,
                file_bytes: "11".to_string(),
                sha256: sha256(format!("subject-tree-{label}")),
            },
            ownership: RunnerPurgeInventoryEvidence {
                entry_count: 1,
                file_bytes: "5".to_string(),
                sha256: sha256(format!("ownership-{label}")),
            },
            target: RunnerPurgeInventoryEvidence {
                entry_count: 3,
                file_bytes: "16".to_string(),
                sha256: sha256(format!("subject-target-{label}")),
            },
            complete_root: RunnerPurgeInventoryEvidence {
                entry_count: 5,
                file_bytes: "16".to_string(),
                sha256: sha256(format!("complete-root-{label}")),
            },
        };
        evidence
    }

    fn fixed_volume(pool: &DbPool, seed_byte: u8, label: &str, enrolled_at_ms: i64) -> FixedVolume {
        fixed_volume_with_legacy_count(pool, seed_byte, label, enrolled_at_ms, 0)
    }

    fn fixed_volume_with_legacy_count(
        pool: &DbPool,
        seed_byte: u8,
        label: &str,
        enrolled_at_ms: i64,
        legacy_artifact_count: i64,
    ) -> FixedVolume {
        let signing_key = Ed25519SigningKey::from_bytes(&[seed_byte; 32]);
        let public_key_base64url = encode_base64url(signing_key.verifying_key().as_bytes());
        let volume_id =
            runner_volume_id_from_public_key(&public_key_base64url).expect("derive test volume id");
        let key_fingerprint = runner_volume_key_fingerprint(&public_key_base64url)
            .expect("derive test key fingerprint");
        let provider_resource_id = format!("resource-{label}");
        let resource_fingerprint = sha256(provider_resource_id.as_bytes());
        let token = encode_base64url(&[seed_byte.wrapping_add(37); 32]);
        let grant = NewRunnerVolumeAdmissionGrant {
            grant_id: format!("grant-{label}"),
            token: token.clone(),
            expected_worker_id: format!("worker-{label}"),
            provider: "test-provider".to_string(),
            provider_resource_id: provider_resource_id.clone(),
            resource_fingerprint: resource_fingerprint.clone(),
            authorization_ref: format!("authorization-{label}"),
            created_by: "test-operator".to_string(),
            expires_at_ms: 1_000_000,
            created_at_ms: enrolled_at_ms.saturating_sub(1),
        };
        let created_grant = create_runner_volume_admission_grant(pool, &grant)
            .expect("create volume admission grant");
        assert_eq!(
            created_grant.disposition,
            RunnerVolumeWriteDisposition::Applied
        );
        let replayed_grant = create_runner_volume_admission_grant(pool, &grant)
            .expect("replay volume admission grant");
        assert_eq!(
            replayed_grant.disposition,
            RunnerVolumeWriteDisposition::Replay
        );
        let proof = sign_runner_volume_enrollment_proof(
            &signing_key,
            NewRunnerVolumeEnrollmentProof {
                admission_grant_id: grant.grant_id,
                volume_id: volume_id.clone(),
                worker_id: grant.expected_worker_id,
                provider: grant.provider,
                provider_resource_id: provider_resource_id.clone(),
                resource_fingerprint: resource_fingerprint.clone(),
                enrollment_epoch: 1,
                public_key_base64url: public_key_base64url.clone(),
                key_fingerprint: key_fingerprint.clone(),
                legacy_artifact_count,
                requested_at_ms: enrolled_at_ms,
            },
        )
        .expect("sign enrollment proof");
        let enrollment = EnrollRunnerVolumeRequest {
            grant_token: token,
            proof,
            enrolled_at_ms,
        };
        let created = enroll_runner_volume(pool, &enrollment).expect("enroll fixed volume");
        assert_eq!(created.disposition, RunnerVolumeWriteDisposition::Applied);
        let mut retry = enrollment.clone();
        retry.enrolled_at_ms = retry.enrolled_at_ms.saturating_add(5);
        let replay = enroll_runner_volume(pool, &retry).expect("replay fixed enrollment");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replay.enrolled_at_ms, enrolled_at_ms);
        let mut tampered = retry;
        tampered.proof.legacy_artifact_count = legacy_artifact_count.saturating_add(1);
        assert!(matches!(
            enroll_runner_volume(pool, &tampered),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
        FixedVolume {
            signing_key,
            volume_id,
            key_fingerprint,
            provider_resource_id,
            resource_fingerprint,
            process_instance_id: encode_base64url(&[seed_byte.wrapping_add(73); 32]),
        }
    }

    fn claim_fixed_volume(pool: &DbPool, volume: &FixedVolume, now_ms: i64) {
        claim_runner_volume_instance(
            pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            now_ms,
            900_000,
        )
        .expect("claim fixed volume instance");
        let current = lookup_runner_volume(pool, &volume.volume_id)
            .expect("load claimed fixed volume")
            .expect("claimed fixed volume exists");
        if current.required_tombstone_generation == current.reconciled_tombstone_generation {
            attest_fixed_volume(pool, volume, now_ms);
        }
    }

    fn attest_fixed_volume(
        pool: &DbPool,
        volume: &FixedVolume,
        now_ms: i64,
    ) -> RunnerVolumeStorageAttestationOutcome {
        let attestation = signed_fixed_storage_attestation(pool, volume, now_ms);
        let next_generation = attestation
            .predecessor_attestation_generation
            .checked_add(1)
            .expect("bounded fixed attestation generation");
        let authority = fixed_storage_attestation_authority(
            pool,
            volume,
            &attestation,
            &format!(
                "storage-authority-{}-{next_generation}-{now_ms}",
                &volume.key_fingerprint[..12]
            ),
            now_ms,
        );
        record_runner_volume_storage_attestation(
            pool,
            &attestation,
            TEST_RUNNER_BUILD,
            now_ms,
            &authority,
        )
        .expect("record fixed storage attestation")
    }

    fn signed_fixed_storage_attestation(
        pool: &DbPool,
        volume: &FixedVolume,
        now_ms: i64,
    ) -> RunnerVolumeStorageAttestation {
        let current = lookup_runner_volume(pool, &volume.volume_id)
            .expect("load fixed volume before storage attestation")
            .expect("fixed volume exists");
        let predecessor = runner_volume_storage_attestation_predecessor(
            pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            now_ms,
        )
        .expect("load fixed storage attestation predecessor");
        let next_generation = predecessor
            .predecessor_attestation_generation
            .checked_add(1)
            .expect("bounded fixed attestation generation");
        sign_runner_volume_storage_attestation(
            &volume.signing_key,
            NewRunnerVolumeStorageAttestation {
                attestation_id: format!(
                    "storage-attestation-{}-{next_generation}",
                    &volume.key_fingerprint[..16]
                ),
                volume_id: volume.volume_id.clone(),
                volume_key_fingerprint: volume.key_fingerprint.clone(),
                resource_fingerprint: volume.resource_fingerprint.clone(),
                enrollment_epoch: 1,
                enrollment_generation: current.enrollment_generation,
                process_instance_id: volume.process_instance_id.clone(),
                predecessor_attestation_generation: predecessor.predecessor_attestation_generation,
                predecessor_attestation_sha256: predecessor.predecessor_attestation_sha256,
                required_tombstone_generation: current.required_tombstone_generation,
                reconciled_tombstone_generation: current.reconciled_tombstone_generation,
                storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
                subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
                root_device_id: "unix:602:test-device".to_string(),
                root_link_count: 7,
                root_entry_count: 0,
                root_file_bytes: "0".to_string(),
                root_sha256: EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
                subject_storage_subject_count: 0,
                subject_storage_subject_set_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                    .to_string(),
                subject_storage_scope_count: 0,
                subject_storage_complete_root_entry_count: 0,
                subject_storage_complete_root_file_bytes: "0".to_string(),
                subject_storage_complete_root_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                    .to_string(),
                locator_count: 0,
                resident_locator_count: 0,
                locator_set_sha256: EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
                legacy_inventory_version: RUNNER_LEGACY_INVENTORY_VERSION,
                legacy_artifact_count: 0,
                legacy_artifact_bytes: "0".to_string(),
                legacy_artifact_set_sha256: EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.to_string(),
                unclassified_root_count: 0,
                runner_build_id: TEST_RUNNER_BUILD.to_string(),
                observed_at_ms: now_ms,
            },
        )
        .expect("sign fixed storage attestation")
    }

    fn fixed_storage_attestation_authority(
        pool: &DbPool,
        volume: &FixedVolume,
        attestation: &RunnerVolumeStorageAttestation,
        request_id: &str,
        now_ms: i64,
    ) -> VerifiedRunnerVolumeAuthority {
        let attestation_sha256 = attestation
            .attestation_sha256()
            .expect("hash fixed storage attestation");
        let proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "storage_attestation".to_string(),
                request_id: request_id.to_string(),
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                issued_at_ms: now_ms,
                payload_sha256: attestation_sha256.clone(),
            },
        )
        .expect("sign fixed storage attestation authority");
        verify_runner_volume_authority_proof(
            pool,
            &proof,
            "storage_attestation",
            &attestation_sha256,
            now_ms,
            90_000,
        )
        .expect("verify fixed storage attestation authority")
    }

    fn resign_fixed_storage_attestation(
        volume: &FixedVolume,
        mut attestation: RunnerVolumeStorageAttestation,
        mutate: impl FnOnce(&mut RunnerVolumeStorageAttestation),
    ) -> RunnerVolumeStorageAttestation {
        mutate(&mut attestation);
        attestation.signature.clear();
        attestation.signature = encode_base64url(
            &volume
                .signing_key
                .sign(
                    &attestation
                        .canonical_unsigned_bytes()
                        .expect("canonicalize mutated storage attestation"),
                )
                .to_bytes(),
        );
        attestation
    }

    fn runner_execution_lease_fixture(
        pool: &DbPool,
        account_id: &str,
        email: &str,
        suffix: &str,
    ) -> (String, String, String) {
        let identity = ensure_primary_application_identity(pool, account_id, email)
            .expect("create runner test identity");
        let now = now_ms();
        let source = ResumeSourceAsset {
            id: format!("resume-source-{account_id}"),
            file_name: "fixture-source-resume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: format!("accounts/{account_id}/jobs/fixture-source-resume.pdf"),
            sha256: "e".repeat(64),
            size_bytes: 1_024,
            page_count: Some(1),
            template_status: "converted_layout".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut profile = default_profile(email);
        profile.onboarding_complete = true;
        profile.source_resume_name = source.file_name.clone();
        profile.source_resume_asset_id = source.id.clone();
        profile.source_resume_sha256 = source.sha256.clone();
        profile.source_resume_media_type = source.media_type.clone();
        profile.source_resume_template_status = source.template_status.clone();
        let (_, profile) = save_resume_source_asset(pool, account_id, &source, &profile)
            .expect("save runner test source resume");
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        let preferences =
            save_preferences(pool, account_id, &preferences).expect("save runner test preferences");
        let track_id = format!("track-{suffix}");
        let track = upsert_track(
            pool,
            account_id,
            &CareerTrack {
                id: track_id.clone(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect("create runner test track");
        assert_eq!(track.policy.authority.review_state, "approved");
        assert!(track.policy.authority.policy_revision_no > 0);
        set_entitlement_plan(pool, account_id, "cloud")
            .expect("enable runner test cloud entitlement");
        let checked_at_ms = now_ms();
        let greenhouse_tenant = format!("acme-{suffix}");
        let canonical_url =
            format!("https://boards.greenhouse.io/{greenhouse_tenant}/jobs/{suffix}");
        let mut posting_input = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse_import".to_string(),
            external_id: suffix.to_string(),
            company: format!("Acme {suffix}"),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url,
            description: concat!(
                "Build reliable products with Rust and TypeScript. Requires 1+ years of ",
                "software engineering experience."
            )
            .to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            track_id,
            match_score: 90,
            matched_reasons: vec!["Skills fit".to_string()],
            missing_requirements: Vec::new(),
            posted_at_ms: Some(checked_at_ms),
            last_verified_at_ms: Some(checked_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        posting_input.canonical_key = canonical_job_key(&posting_input);
        let (posting, managed) = save_production_positive_verified_import(
            pool,
            account_id,
            &posting_input,
            &profile,
            &preferences,
        );
        let authority_fixture = install_production_positive_job_authorities_for_runner(
            pool,
            account_id,
            &posting,
            &managed,
            &format!("{greenhouse_tenant}.example"),
            suffix,
            "cloud",
        );
        let (application, _) =
            prepare_application(pool, account_id, &posting.id, "factual", "review_first")
                .expect("prepare runner test application");
        let run_id = format!("cloud-run-{suffix}");
        upsert_browser_session(
            pool,
            account_id,
            &BrowserSession {
                id: run_id.clone(),
                runner: "cloud".to_string(),
                status: "queued".to_string(),
                current_company: authority_fixture.posting.company.clone(),
                current_step: "Waiting for a browser".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect("create runner test browser session");
        let application = assign_application_run(pool, account_id, &application.id, &run_id)
            .expect("assign runner test run")
            .expect("assigned runner test application exists");
        let authority = current_application_approval_authority(
            pool,
            account_id,
            email,
            &application.id,
        )
        .expect("load current runner test application approval authority")
        .expect("runner test application approval authority exists");
        assert_eq!(authority.posting.id, authority_fixture.posting.id);
        let identity_id = authority
            .application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .expect("runner application identity id")
            .to_string();
        let identity_email = authority
            .application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .expect("runner application identity email")
            .to_string();
        let browser_profile_id = execution_browser_profile_id(account_id, &identity_id);
        let resume = get_resume_version(
            pool,
            account_id,
            authority
                .application
                .resume_version_id
                .as_deref()
                .expect("runner fixture resume id"),
        )
        .expect("load runner fixture resume")
        .expect("runner fixture resume exists");
        let approved_packet = json!({
            "applicationId": authority.application.id,
            "jobId": authority.posting.id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": authority.application.cover_letter,
            "answers": {},
            "verifiedClaimIds": resume.claim_ids,
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": browser_profile_id,
        });
        let approved_job = json!({
            "externalId": authority.posting.external_id,
            "canonicalUrl": authority.posting.canonical_url,
            "company": authority.posting.company,
            "title": authority.posting.title,
            "location": authority.posting.location,
            "workplace": authority.posting.workplace,
            "description": authority.posting.description,
            "source": authority.posting.source,
            "compensation": authority.posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .expect("checksum runner fixture approval");
        let approved_execution = json!({
            "schema_version": 2,
            "approved_at_ms": authority.evaluated_at_ms,
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        let application = persist_current_application_approval(
            pool,
            account_id,
            email,
            &authority,
            &approved_execution,
        )
        .expect("persist current runner test application approval")
        .expect("runner test application remains available for approval");
        reserve_application_attempt(pool, account_id, &application.id, "unassigned")
            .expect("reserve unassigned runner test attempt");
        let application = update_application(pool, account_id, &application.id, "queued", None)
            .expect("queue runner test application")
            .expect("runner test application remains available for queueing");
        (application.id, run_id, browser_profile_id)
    }

    fn server_signer() -> RunnerPurgeSigner {
        RunnerPurgeSigner::from_seed("server-key-1", [7_u8; 32])
            .expect("construct fixed server signer")
    }

    fn server_key_ring(signer: &RunnerPurgeSigner) -> RunnerPurgeCommandKeyRing {
        RunnerPurgeCommandKeyRing::new(
            signer,
            BTreeMap::from([(signer.key_id().to_string(), signer.public_key_base64url())]),
        )
        .expect("construct fixed server key ring")
    }

    fn ensure_legacy_inventory_ready(pool: &DbPool, label: &str, at_ms: i64) {
        let status = runner_volume_fleet_status(pool).expect("load legacy inventory state");
        if status.legacy_inventory_state == "ready" {
            return;
        }
        assert!(matches!(
            status.legacy_inventory_state.as_str(),
            "unknown" | "reconciling"
        ));
        let reconciling = record_runner_legacy_inventory_authority(
            pool,
            &RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: format!("inventory-{label}"),
                authority_state: "reconciling".to_string(),
                expected_predecessor_generation: status.legacy_inventory_generation,
                expected_predecessor_authority_id: status.legacy_inventory_authority_id,
                expected_predecessor_authority_sha256: status.legacy_inventory_authority_sha256,
                root_count: 0,
                root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
                scope_ref: "all-managed-runner-storage-roots".to_string(),
                evidence_ref: format!("inventory-start-{label}"),
                evidence_sha256: sha256(format!("inventory-start-evidence-{label}")),
                authorized_by: "test-operator".to_string(),
                recorded_at_ms: at_ms,
            },
        )
        .expect("begin legacy inventory reconciliation")
        .authority;
        record_runner_legacy_inventory_authority(
            pool,
            &RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: reconciling.reconciliation_id.clone(),
                authority_state: "ready".to_string(),
                expected_predecessor_generation: reconciling.authority_generation,
                expected_predecessor_authority_id: Some(reconciling.authority_id.clone()),
                expected_predecessor_authority_sha256: Some(reconciling.authority_sha256.clone()),
                root_count: reconciling.root_count,
                root_set_sha256: reconciling.root_set_sha256,
                scope_ref: reconciling.scope_ref,
                evidence_ref: format!("inventory-ready-{label}"),
                evidence_sha256: sha256(format!("inventory-ready-evidence-{label}")),
                authorized_by: "test-operator".to_string(),
                recorded_at_ms: at_ms.saturating_add(1),
            },
        )
        .expect("complete legacy inventory reconciliation");
    }

    fn advance_legacy_inventory_ready(pool: &DbPool, label: &str, at_ms: i64) {
        let current = runner_volume_fleet_status(pool).expect("load current legacy authority");
        assert_eq!(current.legacy_inventory_state, "ready");
        let reconciling = record_runner_legacy_inventory_authority(
            pool,
            &RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: format!("inventory-{label}"),
                authority_state: "reconciling".to_string(),
                expected_predecessor_generation: current.legacy_inventory_generation,
                expected_predecessor_authority_id: current.legacy_inventory_authority_id,
                expected_predecessor_authority_sha256: current.legacy_inventory_authority_sha256,
                root_count: 0,
                root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
                scope_ref: "all-managed-runner-storage-roots".to_string(),
                evidence_ref: format!("inventory-start-{label}"),
                evidence_sha256: sha256(format!("inventory-start-evidence-{label}")),
                authorized_by: "test-operator".to_string(),
                recorded_at_ms: at_ms,
            },
        )
        .expect("begin successor legacy inventory reconciliation")
        .authority;
        record_runner_legacy_inventory_authority(
            pool,
            &RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: reconciling.reconciliation_id.clone(),
                authority_state: "ready".to_string(),
                expected_predecessor_generation: reconciling.authority_generation,
                expected_predecessor_authority_id: Some(reconciling.authority_id.clone()),
                expected_predecessor_authority_sha256: Some(reconciling.authority_sha256.clone()),
                root_count: reconciling.root_count,
                root_set_sha256: reconciling.root_set_sha256,
                scope_ref: reconciling.scope_ref,
                evidence_ref: format!("inventory-ready-{label}"),
                evidence_sha256: sha256(format!("inventory-ready-evidence-{label}")),
                authorized_by: "test-operator".to_string(),
                recorded_at_ms: at_ms.saturating_add(1),
            },
        )
        .expect("complete successor legacy inventory reconciliation");
    }

    fn mark_current_fleet_ready(pool: &DbPool, label: &str, at_ms: i64) {
        ensure_legacy_inventory_ready(pool, label, at_ms.saturating_sub(2));
        let status = runner_volume_fleet_status(pool).expect("load fleet before cutover");
        let mut request = RecordRunnerVolumeFleetCutoverRequest {
            cutover_state: "reconciling".to_string(),
            expected_enrollment_generation: status.enrollment_generation,
            expected_purge_generation: status.purge_generation,
            expected_tombstone_generation: status.tombstone_generation,
            expected_destruction_generation: status.destruction_generation,
            expected_legacy_reconciliation_generation: status.legacy_reconciliation_generation,
            expected_storage_attestation_generation: status.storage_attestation_generation,
            expected_storage_attestation_count: status.storage_attestation_count,
            expected_storage_attestation_set_sha256: status.storage_attestation_set_sha256.clone(),
            expected_legacy_inventory_generation: status.legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: status
                .legacy_inventory_reconciliation_id
                .clone()
                .expect("ready legacy inventory reconciliation id"),
            expected_legacy_inventory_authority_id: status
                .legacy_inventory_authority_id
                .clone()
                .expect("ready legacy inventory authority id"),
            expected_legacy_inventory_authority_sha256: status
                .legacy_inventory_authority_sha256
                .clone()
                .expect("ready legacy inventory authority digest"),
            expected_legacy_inventory_root_count: status
                .legacy_inventory_root_count
                .expect("ready legacy inventory root count"),
            expected_legacy_inventory_root_set_sha256: status
                .legacy_inventory_root_set_sha256
                .clone()
                .expect("ready legacy inventory root set digest"),
            expected_non_destroyed_volume_count: status.non_destroyed_volume_count,
            expected_destruction_count: status.destruction_count,
            expected_unresolved_legacy_volume_count: status.unresolved_legacy_volume_count,
            evidence_ref: format!("cutover-{label}"),
            evidence_sha256: sha256(format!("cutover-evidence-{label}")),
            authorized_by: "test-operator".to_string(),
            cutover_at_ms: at_ms,
            now_ms: at_ms,
        };
        record_runner_volume_fleet_cutover(pool, &request).expect("freeze current fleet snapshot");
        request.cutover_state = "ready".to_string();
        request.now_ms = at_ms.saturating_add(1);
        record_runner_volume_fleet_cutover(pool, &request).expect("mark current fleet ready");
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_certified_cloud_runtime_fixture(
        pool: &DbPool,
        account_id: &str,
        application_id: &str,
        run_id: &str,
        browser_profile_id: &str,
        owner_id: &str,
        runtime: RunnerProcessRuntimeAttestation,
        now_ms: i64,
    ) -> RunnerVolumePurgeResult<InstalledCertifiedCloudRuntimeFixture> {
        for value in [account_id, application_id, run_id, owner_id] {
            require_nonempty_text(value, 240)?;
        }
        require_nonempty_text(browser_profile_id, 160)?;
        let claim_now_ms = now_ms
            .checked_sub(4)
            .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
        let runtime_expires_at_ms = now_ms
            .checked_add(900_000)
            .ok_or(RunnerVolumePurgeError::InvalidRequest)?;
        let runtime_sha256 = runner_process_runtime_sha256(&runtime)?;
        let fixture_context = format!(
            concat!(
                "certified-cloud-runtime-fixture-v1\n",
                "account_id={}\n",
                "application_id={}\n",
                "run_id={}\n",
                "browser_profile_id={}\n",
                "owner_id={}\n",
                "runtime_sha256={}\n"
            ),
            account_id, application_id, run_id, browser_profile_id, owner_id, runtime_sha256,
        );
        let fixture_sha256 = sha256(fixture_context.as_bytes());
        let fixture_label = format!("certified-cloud-{}", &fixture_sha256[..16]);
        let seed_byte = Sha256::digest(fixture_context.as_bytes())[0];
        let volume = fixed_volume(pool, seed_byte, &fixture_label, 10);
        let worker_id = format!("worker-{fixture_label}");

        let runtime_grant_id = format!("runtime-grant-{fixture_label}");
        let runtime_token = encode_base64url(&[seed_byte.wrapping_add(101); 32]);
        let runtime_grant = create_runner_process_runtime_grant(
            pool,
            &NewRunnerProcessRuntimeGrant {
                grant_id: runtime_grant_id.clone(),
                token: runtime_token.clone(),
                expected_worker_id: worker_id.clone(),
                runtime: runtime.clone(),
                authorization_ref: format!("deployment-{fixture_label}"),
                created_by: "test-operator".to_string(),
                expires_at_ms: runtime_expires_at_ms,
                created_at_ms: claim_now_ms.saturating_sub(1),
            },
        )?;
        if runtime_grant.runtime_sha256 != runtime_sha256 {
            return Err(RunnerVolumePurgeError::Conflict);
        }

        let instance_claim_path = format!(
            "/api/jobs/internal/runner-volumes/{}/instances/claim",
            volume.volume_id
        );
        let runtime_grant_token_sha256 = hex::encode(Sha256::digest(runtime_token.as_bytes()));
        let instance_claim_payload_sha256 =
            crate::api::jobs_runner_volumes::runner_volume_http_payload_sha256(
                &instance_claim_path,
                &worker_id,
                &[
                    ("runtime_grant_id", runtime_grant_id.as_str()),
                    (
                        "runtime_grant_token_sha256",
                        runtime_grant_token_sha256.as_str(),
                    ),
                    ("runtime_sha256", runtime_sha256.as_str()),
                ],
            );
        let instance_claim_proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "instance_claim".to_string(),
                request_id: format!("instance-claim-{fixture_label}"),
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                issued_at_ms: claim_now_ms,
                payload_sha256: instance_claim_payload_sha256.clone(),
            },
        )?;
        let instance_claim_authority = verify_runner_volume_authority_proof(
            pool,
            &instance_claim_proof,
            "instance_claim",
            &instance_claim_payload_sha256,
            claim_now_ms,
            90_000,
        )?;
        let instance_lease = claim_runner_volume_instance_authorized(
            pool,
            &RunnerVolumeInstanceLeaseRequest {
                volume_id: &volume.volume_id,
                enrollment_epoch: 1,
                process_instance_id: &volume.process_instance_id,
                now_ms: claim_now_ms,
                lease_expires_at_ms: runtime_expires_at_ms,
            },
            &RunnerProcessRuntimeGrantClaim {
                grant_id: runtime_grant_id.clone(),
                grant_token: runtime_token,
                runtime: runtime.clone(),
            },
            &instance_claim_authority,
        )?;
        if instance_lease.runtime_grant_id.as_deref() != Some(runtime_grant_id.as_str())
            || instance_lease.runtime_sha256.as_deref() != Some(runtime_sha256.as_str())
        {
            return Err(RunnerVolumePurgeError::Conflict);
        }

        let attested_at_ms = claim_now_ms.saturating_add(1);
        attest_fixed_volume(pool, &volume, attested_at_ms);
        activate_reconciled_runner_volume(
            pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            attested_at_ms,
        )?;
        mark_current_fleet_ready(pool, &fixture_label, claim_now_ms.saturating_add(2));

        let binding_request = BindRunnerVolumeResidencyRequest {
            account_id: account_id.to_string(),
            run_id: run_id.to_string(),
            worker_id,
            volume_id: volume.volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: volume.process_instance_id.clone(),
            now_ms,
        };
        let execution_claim_payload_sha256 =
            crate::api::jobs_runner_volumes::runner_volume_execution_lease_claim_payload_sha256(
                &crate::api::jobs_runner_volumes::RunnerVolumeExecutionLeaseClaimPayload {
                    worker_id: &binding_request.worker_id,
                    account_id,
                    application_id,
                    run_id,
                    browser_profile_id,
                    owner_id,
                    volume_id: &binding_request.volume_id,
                    enrollment_epoch: binding_request.enrollment_epoch,
                    process_instance_id: &binding_request.process_instance_id,
                    runtime_grant_id: &runtime_grant_id,
                    runtime_sha256: &runtime_sha256,
                    managed_cloud: None,
                },
            );
        let execution_claim_proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "execution_lease_claim".to_string(),
                request_id: format!("execution-lease-claim-{fixture_label}"),
                volume_id: volume.volume_id,
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id,
                issued_at_ms: now_ms,
                payload_sha256: execution_claim_payload_sha256.clone(),
            },
        )?;
        let verified_authority = verify_runner_volume_authority_proof(
            pool,
            &execution_claim_proof,
            "execution_lease_claim",
            &execution_claim_payload_sha256,
            now_ms,
            90_000,
        )?;

        let runtime_target = AtsCertificationRuntimeTarget {
            runtime_kind: "cloud".to_string(),
            runtime_id: format!("cloud:{}", runtime.runner_build_id),
            runtime_sha256: runtime_sha256.clone(),
            platform: runtime.platform,
            architecture: runtime.architecture,
            automation_bundle_sha256: runtime.automation_bundle_sha256,
            browser_release_manifest_sha256: None,
            browser_artifact_sha256: None,
            browser_build_descriptor_sha256: None,
            runner_build_id: Some(runtime.runner_build_id),
            runner_image_sha256: Some(runtime.runner_image_sha256),
            playwright_version: runtime.playwright_version,
            chromium_revision: runtime.chromium_revision,
            chromium_executable_sha256: runtime.chromium_executable_sha256,
        };
        validate_ats_runtime_target(&runtime_target)
            .map_err(|_| RunnerVolumePurgeError::InvalidRequest)?;
        Ok(InstalledCertifiedCloudRuntimeFixture {
            runtime_target,
            binding_request,
            runtime_grant_id,
            runtime_sha256,
            verified_authority,
        })
    }

    #[test]
    fn certified_cloud_runtime_fixture_authorizes_exact_execution_lease_claim() {
        let pool = super::tests::test_pool();
        let owner_id = "round604-certified-cloud-owner";
        let runtime = RunnerProcessRuntimeAttestation {
            runner_image_sha256: sha256("certified-cloud-image"),
            runner_build_id: "runner-604.1".to_string(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: sha256("certified-cloud-automation"),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "123456".to_string(),
            chromium_executable_sha256: sha256("certified-cloud-chromium"),
        };
        let runtime_target = super::tests::certified_cloud_runtime_target(&runtime);
        let fixture = super::tests::certified_application_fixture(
            &pool,
            "certified-cloud-runtime",
            "cloud",
            runtime_target.clone(),
            "",
        );
        let application_id = fixture.application.id;
        let run_id = fixture.run_id;
        let browser_profile_id = fixture.browser_profile_id;
        let now = now_ms();
        let installed = install_certified_cloud_runtime_fixture(
            &pool,
            "acct-jobs",
            &application_id,
            &run_id,
            &browser_profile_id,
            owner_id,
            runtime.clone(),
            now,
        )
        .expect("install exact certified cloud runtime fixture");
        assert_eq!(installed.runtime_target, runtime_target);
        assert_eq!(
            installed.runtime_target.runner_image_sha256.as_deref(),
            Some(runtime.runner_image_sha256.as_str())
        );
        assert_eq!(
            installed.runtime_target.runtime_sha256,
            installed.runtime_sha256
        );
        assert_eq!(installed.binding_request.account_id, "acct-jobs");
        assert_eq!(installed.binding_request.run_id, run_id);

        let grant = claim_execution_lease_for_runner_volume_authorized(
            &pool,
            "acct-jobs",
            &application_id,
            &run_id,
            &browser_profile_id,
            owner_id,
            &installed.binding_request,
            &installed.runtime_grant_id,
            &installed.runtime_sha256,
            &installed.verified_authority,
        )
        .expect("claim lease through exact runtime and volume authority");
        assert_eq!(grant.runtime_grant_id, installed.runtime_grant_id);
        assert_eq!(grant.runtime_sha256, installed.runtime_sha256);
        assert_eq!(grant.volume_id, installed.binding_request.volume_id);
    }

    fn prepare(
        database: &TestDatabase,
        signer: &RunnerPurgeSigner,
        account_id: &str,
        request_id: &str,
        now_ms: i64,
        subject_byte: u8,
    ) -> PreparedRunnerVolumePurge {
        database.fence_account(account_id, now_ms);
        ensure_legacy_inventory_ready(&database.pool, request_id, now_ms.saturating_sub(2));
        let fleet = runner_volume_fleet_status(&database.pool)
            .expect("load ready legacy inventory for purge");
        let key_ring = server_key_ring(signer);
        let prepared = prepare_runner_volume_purge_with_subject_material(
            &database.pool,
            signer,
            &key_ring,
            &PrepareRunnerVolumePurgeRequest {
                request_id: request_id.to_string(),
                account_id: account_id.to_string(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                expected_legacy_inventory_generation: fleet.legacy_inventory_generation,
                expected_legacy_inventory_reconciliation_id: fleet
                    .legacy_inventory_reconciliation_id
                    .expect("legacy inventory reconciliation id"),
                expected_legacy_inventory_authority_id: fleet
                    .legacy_inventory_authority_id
                    .expect("legacy inventory authority id"),
                expected_legacy_inventory_authority_sha256: fleet
                    .legacy_inventory_authority_sha256
                    .expect("legacy inventory authority digest"),
                now_ms,
            },
            [subject_byte; 32],
        )
        .expect("prepare runner-volume purge");
        for command in &prepared.commands {
            assert_eq!(
                command.storage_evidence_version,
                RUNNER_PURGE_STORAGE_EVIDENCE_VERSION
            );
            assert_eq!(
                command.subject_storage_layout_version,
                RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION
            );
            assert_eq!(
                command.legacy_inventory_authority_generation,
                prepared.status.legacy_inventory_generation
            );
            assert_eq!(
                command.legacy_inventory_authority_sha256,
                prepared.status.legacy_inventory_authority_sha256
            );
        }
        prepared
    }

    fn signed_ack(
        volume: &FixedVolume,
        command: &RunnerPurgeCommand,
        completed_at_ms: i64,
    ) -> RunnerPurgeAck {
        let storage_evidence = test_storage_evidence(&command.command_id, 1);
        let storage_evidence_sha256 = storage_evidence
            .sha256()
            .expect("hash fixed storage evidence");
        let before_inventory = storage_evidence
            .target_inventory_state(true)
            .expect("derive fixed before inventory");
        let after_inventory = storage_evidence
            .target_inventory_state(false)
            .expect("derive fixed after inventory");
        sign_runner_purge_ack(
            &volume.signing_key,
            NewRunnerPurgeAck {
                request_id: command.request_id.clone(),
                command_id: command.command_id.clone(),
                command_sha256: command.command_sha256().expect("hash command"),
                target_volume_id: command.target_volume_id.clone(),
                target_key_fingerprint: command.target_key_fingerprint.clone(),
                enrollment_epoch: command.enrollment_epoch,
                process_instance_id: volume.process_instance_id.clone(),
                purge_subject_sha256: runner_purge_subject_sha256(&command.purge_subject)
                    .expect("hash purge subject"),
                purge_generation: command.purge_generation,
                storage_evidence,
                storage_evidence_sha256,
                before_inventory_count: before_inventory.0,
                before_inventory_sha256: before_inventory.1,
                after_inventory_count: after_inventory.0,
                after_inventory_sha256: after_inventory.1,
                removed_count: before_inventory.0 - after_inventory.0,
                runner_build_id: TEST_RUNNER_BUILD.to_string(),
                completed_at_ms,
            },
        )
        .expect("sign fixed purge acknowledgement")
    }

    fn command_for<'a>(
        prepared: &'a PreparedRunnerVolumePurge,
        volume: &FixedVolume,
    ) -> &'a RunnerPurgeCommand {
        prepared
            .commands
            .iter()
            .find(|command| command.target_volume_id == volume.volume_id)
            .expect("prepared command for volume")
    }

    #[test]
    fn legacy_inventory_authority_is_fail_closed_append_only_and_two_phase() {
        let database = TestDatabase::new(&[]);
        let fresh = runner_volume_fleet_status(&database.pool).expect("load fresh fleet");
        assert_eq!(fresh.legacy_inventory_state, "unknown");
        assert_eq!(fresh.legacy_inventory_generation, 0);
        assert!(fresh.legacy_inventory_authority_id.is_none());
        assert!(fresh.legacy_inventory_root_count.is_none());

        let direct_ready = RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: "inventory-direct-ready".to_string(),
            authority_state: "ready".to_string(),
            expected_predecessor_generation: 0,
            expected_predecessor_authority_id: None,
            expected_predecessor_authority_sha256: None,
            root_count: 0,
            root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            scope_ref: "all-managed-runner-storage-roots".to_string(),
            evidence_ref: "direct-ready-evidence".to_string(),
            evidence_sha256: sha256(b"direct-ready-evidence"),
            authorized_by: "test-operator".to_string(),
            recorded_at_ms: 1,
        };
        assert!(matches!(
            record_runner_legacy_inventory_authority(&database.pool, &direct_ready),
            Err(RunnerVolumePurgeError::NotReady)
        ));

        let mut reconciling = direct_ready;
        reconciling.reconciliation_id = "inventory-authority-chain".to_string();
        reconciling.authority_state = "reconciling".to_string();
        reconciling.evidence_ref = "inventory-scan-start".to_string();
        reconciling.evidence_sha256 = sha256(b"inventory-scan-start");
        let mut noncanonical_empty = reconciling.clone();
        noncanonical_empty.root_set_sha256 = sha256(b"not-the-canonical-empty-root-set");
        assert!(matches!(
            record_runner_legacy_inventory_authority(&database.pool, &noncanonical_empty),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));

        let started = record_runner_legacy_inventory_authority(&database.pool, &reconciling)
            .expect("start exact legacy inventory reconciliation");
        assert_eq!(started.disposition, RunnerVolumeWriteDisposition::Applied);
        assert_eq!(started.authority.authority_generation, 1);
        let reconciling_status =
            runner_volume_fleet_status(&database.pool).expect("load reconciling fleet");
        assert_eq!(reconciling_status.legacy_inventory_state, "reconciling");
        assert_eq!(reconciling_status.legacy_inventory_generation, 1);

        let mut replay = reconciling.clone();
        replay.recorded_at_ms = 50;
        let replayed = record_runner_legacy_inventory_authority(&database.pool, &replay)
            .expect("replay exact reconciliation with a new server time");
        assert_eq!(replayed.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replayed.authority.recorded_at_ms, 1);
        let mut conflict = replay;
        conflict.evidence_sha256 = sha256(b"conflicting-reconciliation-evidence");
        assert!(matches!(
            record_runner_legacy_inventory_authority(&database.pool, &conflict),
            Err(RunnerVolumePurgeError::Conflict)
        ));

        let ready = RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: started.authority.reconciliation_id.clone(),
            authority_state: "ready".to_string(),
            expected_predecessor_generation: started.authority.authority_generation,
            expected_predecessor_authority_id: Some(started.authority.authority_id.clone()),
            expected_predecessor_authority_sha256: Some(started.authority.authority_sha256.clone()),
            root_count: started.authority.root_count,
            root_set_sha256: started.authority.root_set_sha256.clone(),
            scope_ref: started.authority.scope_ref.clone(),
            evidence_ref: "inventory-scan-complete".to_string(),
            evidence_sha256: sha256(b"inventory-scan-complete"),
            authorized_by: "test-operator".to_string(),
            recorded_at_ms: 2,
        };
        let completed = record_runner_legacy_inventory_authority(&database.pool, &ready)
            .expect("mark exact legacy inventory ready");
        assert_eq!(completed.authority.authority_generation, 2);
        let ready_status = runner_volume_fleet_status(&database.pool).expect("load ready fleet");
        assert_eq!(ready_status.legacy_inventory_state, "ready");
        assert_eq!(ready_status.legacy_inventory_root_count, Some(0));
        assert_eq!(
            ready_status.legacy_inventory_root_set_sha256.as_deref(),
            Some(EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256)
        );

        let conn = database
            .pool
            .get()
            .expect("open append-only test connection");
        assert!(conn
            .execute(
                "UPDATE jobs_runner_legacy_inventory_authorities \
                    SET evidence_ref = 'tampered' WHERE authority_id = ?1",
                params![completed.authority.authority_id],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM jobs_runner_legacy_inventory_authorities WHERE authority_id = ?1",
                params![completed.authority.authority_id],
            )
            .is_err());
    }

    #[test]
    fn postgres_legacy_authority_locks_fleet_before_idempotency_recheck() {
        // The unit harness has no live PostgreSQL dependency. Keep the
        // security-critical ordering visible and regression tested: the
        // singleton allocation lock must precede the exact replay lookup.
        let source = include_str!("runner_volume_purge.rs");
        let function = source
            .split_once("fn record_runner_legacy_inventory_authority_postgres(")
            .expect("PostgreSQL legacy authority function")
            .1
            .split_once("\n#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]")
            .expect("end of PostgreSQL legacy authority function")
            .0;
        let fleet_lock = function
            .find("jobs_runner_volume_fleet_state WHERE singleton_id = 1 FOR UPDATE")
            .expect("fleet singleton allocation lock");
        let replay_lookup = function
            .find("let select_existing = format!")
            .expect("locked idempotency lookup");
        assert!(fleet_lock < replay_lookup);
        assert!(function[replay_lookup..]
            .contains("WHERE reconciliation_id = $1 AND authority_state = $2 FOR UPDATE"));
    }

    #[test]
    fn postgres_instance_claim_locks_fleet_before_volume_and_cutover_invalidation() {
        // The unit harness has no live PostgreSQL dependency. Preserve the
        // global fleet -> child-volume ordering that prevents managed-effect
        // transactions from deadlocking with runner instance claims.
        let source = include_str!("runner_volume_purge.rs");
        let function = source
            .split_once("fn claim_runner_volume_instance_postgres(")
            .expect("PostgreSQL runner instance claim")
            .1
            .split_once("\nfn validate_instance_claim_state(")
            .expect("end of PostgreSQL runner instance claim")
            .0;
        let fleet_locks = function
            .match_indices("jobs_runner_volume_fleet_state")
            .map(|(position, _)| position)
            .collect::<Vec<_>>();
        assert_eq!(fleet_locks.len(), 1, "exactly one fleet UPDATE lock");
        let fleet_lock = fleet_locks[0];
        let volume_lock = function
            .find("jobs_runner_volumes WHERE volume_id = $1 FOR UPDATE")
            .expect("child volume UPDATE lock");
        let invalidate = function
            .find("invalidate_postgres_runner_fleet_cutover")
            .expect("fleet cutover invalidation");
        assert!(fleet_lock < volume_lock);
        assert!(function[fleet_lock..volume_lock].contains("WHERE singleton_id = 1 FOR UPDATE"));
        assert!(volume_lock < invalidate);
        assert!(!function[..fleet_lock].contains("jobs_runner_volumes"));
    }

    #[test]
    fn postgres_fanout_and_ack_replay_keep_conservative_immutable_rows() {
        // PostgreSQL is not required by the unit harness, so preserve the
        // security-critical query shapes as source-level regression guards.
        let source = include_str!("runner_volume_purge.rs");
        let fanout = source
            .split_once("fn load_postgres_frozen_targets(")
            .expect("PostgreSQL frozen-target loader")
            .1
            .split_once("\nfn postgres_legacy_account_storage_count(")
            .expect("end of PostgreSQL frozen-target loader")
            .0;
        assert!(fanout.contains("WHERE v.status <> 'destroyed'"));
        assert!(fanout.contains("FOR UPDATE OF v, k"));
        assert!(!fanout.contains("jobs_runner_volume_storage_attestations"));
        assert!(!fanout.contains("RunnerVolumePurgeError::NotReady"));

        let ack_lookup = source
            .split_once("fn load_postgres_purge_command_for_ack(")
            .expect("PostgreSQL ACK command loader")
            .1
            .split_once("\nfn update_postgres_stored_ack(")
            .expect("end of PostgreSQL ACK command loader")
            .0;
        assert!(ack_lookup.contains("request.state"));
        assert!(!ack_lookup.contains("request.state <> 'superseded'"));

        let acknowledge = source
            .split_once("fn acknowledge_runner_volume_purge_postgres(")
            .expect("PostgreSQL ACK transaction")
            .1
            .split_once("\nfn load_postgres_ack_volume(")
            .expect("end of PostgreSQL ACK transaction")
            .0;
        assert!(acknowledge.contains("stored.state == \"acknowledged\""));
        assert!(acknowledge.contains("stored.request_state.as_deref() != Some(\"pending\")"));
    }

    #[test]
    fn legacy_inventory_drift_blocks_prepare_completion_and_invalidates_cutover() {
        let database = TestDatabase::new(&["acct-drift", "acct-unknown"]);
        database.fence_account("acct-unknown", 1);
        let signer = server_signer();
        let unknown_prepare = PrepareRunnerVolumePurgeRequest {
            request_id: "request-unknown-inventory".to_string(),
            account_id: "acct-unknown".to_string(),
            minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
            expected_legacy_inventory_generation: 1,
            expected_legacy_inventory_reconciliation_id: "missing-inventory".to_string(),
            expected_legacy_inventory_authority_id: "missing-authority".to_string(),
            expected_legacy_inventory_authority_sha256: sha256(b"missing-authority"),
            now_ms: 2,
        };
        assert!(matches!(
            prepare_runner_volume_purge(
                &database.pool,
                &signer,
                &server_key_ring(&signer),
                &unknown_prepare,
            ),
            Err(RunnerVolumePurgeError::NotReady)
        ));

        ensure_legacy_inventory_ready(&database.pool, "drift", 3);
        let prepared = prepare(
            &database,
            &signer,
            "acct-drift",
            "request-inventory-drift",
            8,
            77,
        );
        assert_eq!(prepared.status.required_target_count, 0);
        mark_current_fleet_ready(&database.pool, "drift-after-prepare", 8);
        let ready = runner_volume_fleet_status(&database.pool).expect("load ready inventory");
        assert_eq!(ready.cutover_state, "ready");
        let drift = RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: "inventory-drift-successor".to_string(),
            authority_state: "reconciling".to_string(),
            expected_predecessor_generation: ready.legacy_inventory_generation,
            expected_predecessor_authority_id: ready.legacy_inventory_authority_id.clone(),
            expected_predecessor_authority_sha256: ready.legacy_inventory_authority_sha256.clone(),
            root_count: 0,
            root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
            scope_ref: "all-managed-runner-storage-roots".to_string(),
            evidence_ref: "inventory-drift-start".to_string(),
            evidence_sha256: sha256(b"inventory-drift-start"),
            authorized_by: "test-operator".to_string(),
            recorded_at_ms: 9,
        };
        let drifted = record_runner_legacy_inventory_authority(&database.pool, &drift)
            .expect("record inventory drift")
            .authority;
        let after_drift = runner_volume_fleet_status(&database.pool).expect("load drifted fleet");
        assert_eq!(after_drift.legacy_inventory_state, "reconciling");
        assert_eq!(after_drift.cutover_state, "reconciling");
        assert!(after_drift.cutover_legacy_inventory_generation.is_none());
        assert!(matches!(
            complete_runner_volume_purge(&database.pool, "request-inventory-drift", 10),
            Err(RunnerVolumePurgeError::NotReady)
        ));

        let successor_ready = RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: drifted.reconciliation_id.clone(),
            authority_state: "ready".to_string(),
            expected_predecessor_generation: drifted.authority_generation,
            expected_predecessor_authority_id: Some(drifted.authority_id.clone()),
            expected_predecessor_authority_sha256: Some(drifted.authority_sha256.clone()),
            root_count: drifted.root_count,
            root_set_sha256: drifted.root_set_sha256,
            scope_ref: drifted.scope_ref,
            evidence_ref: "inventory-drift-complete".to_string(),
            evidence_sha256: sha256(b"inventory-drift-complete"),
            authorized_by: "test-operator".to_string(),
            recorded_at_ms: 11,
        };
        record_runner_legacy_inventory_authority(&database.pool, &successor_ready)
            .expect("complete successor inventory");
        assert!(matches!(
            complete_runner_volume_purge(&database.pool, "request-inventory-drift", 12),
            Err(RunnerVolumePurgeError::NotReady)
        ));
        let successor_fleet =
            runner_volume_fleet_status(&database.pool).expect("load successor authority");
        let mut successor_prepare = PrepareRunnerVolumePurgeRequest {
            request_id: "request-inventory-drift".to_string(),
            account_id: "acct-drift".to_string(),
            minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
            expected_legacy_inventory_generation: successor_fleet.legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: successor_fleet
                .legacy_inventory_reconciliation_id
                .expect("successor reconciliation id"),
            expected_legacy_inventory_authority_id: successor_fleet
                .legacy_inventory_authority_id
                .expect("successor authority id"),
            expected_legacy_inventory_authority_sha256: successor_fleet
                .legacy_inventory_authority_sha256
                .expect("successor authority digest"),
            now_ms: 13,
        };
        let successor = prepare_runner_volume_purge(
            &database.pool,
            &signer,
            &server_key_ring(&signer),
            &successor_prepare,
        )
        .expect("prepare authority-bound successor purge");
        assert_eq!(successor.disposition, RunnerVolumeWriteDisposition::Applied);
        assert_ne!(successor.status.request_id, successor_prepare.request_id);
        assert!(successor.status.required_target_count >= prepared.status.required_target_count);
        assert_eq!(
            runner_volume_purge_status(&database.pool, "request-inventory-drift")
                .expect("load superseded purge")
                .state,
            "superseded"
        );
        successor_prepare.now_ms = 14;
        let replay = prepare_runner_volume_purge(
            &database.pool,
            &signer,
            &server_key_ring(&signer),
            &successor_prepare,
        )
        .expect("replay authority-bound successor purge");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replay.status.request_id, successor.status.request_id);
        assert_eq!(
            complete_runner_volume_purge(&database.pool, &successor.status.request_id, 15)
                .expect("complete successor purge")
                .status
                .state,
            "complete"
        );
    }

    #[test]
    fn completed_purge_can_be_reauthorized_without_shrinking_frozen_targets() {
        let database = TestDatabase::new(&["acct-successor"]);
        let signer = server_signer();
        let key_ring = server_key_ring(&signer);
        let first_volume = fixed_volume(&database.pool, 42, "successor-first", 4);
        claim_fixed_volume(&database.pool, &first_volume, 7);
        let first = prepare(
            &database,
            &signer,
            "acct-successor",
            "request-successor-stable",
            10,
            43,
        );
        assert_eq!(first.status.required_target_count, 1);
        let first_ack = signed_ack(&first_volume, command_for(&first, &first_volume), 11);
        acknowledge_runner_volume_purge(&database.pool, &key_ring, &first_ack, 11)
            .expect("acknowledge first purge attempt");
        let first_completion =
            complete_runner_volume_purge(&database.pool, &first.status.request_id, 12)
                .expect("complete first purge attempt");
        assert_eq!(first_completion.status.state, "complete");
        attest_fixed_volume(&database.pool, &first_volume, 13);

        let second_volume = fixed_volume(&database.pool, 44, "successor-second", 22);
        claim_fixed_volume(&database.pool, &second_volume, 23);
        ensure_legacy_inventory_ready(&database.pool, "successor-authority", 24);
        let fleet = runner_volume_fleet_status(&database.pool).expect("load successor fleet");
        let mut request = PrepareRunnerVolumePurgeRequest {
            request_id: "request-successor-stable".to_string(),
            account_id: "acct-successor".to_string(),
            minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
            expected_legacy_inventory_generation: fleet.legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: fleet
                .legacy_inventory_reconciliation_id
                .expect("successor reconciliation id"),
            expected_legacy_inventory_authority_id: fleet
                .legacy_inventory_authority_id
                .expect("successor authority id"),
            expected_legacy_inventory_authority_sha256: fleet
                .legacy_inventory_authority_sha256
                .expect("successor authority digest"),
            now_ms: 26,
        };
        let successor = prepare_runner_volume_purge(&database.pool, &signer, &key_ring, &request)
            .expect("prepare purge successor after completed attempt");
        assert_eq!(successor.disposition, RunnerVolumeWriteDisposition::Applied);
        assert_ne!(successor.status.request_id, first.status.request_id);
        assert_eq!(successor.status.purge_subject, first.status.purge_subject);
        assert_eq!(successor.status.required_target_count, 2);
        assert_eq!(successor.commands.len(), 2);
        assert_eq!(
            runner_volume_purge_status(&database.pool, &first.status.request_id)
                .expect("load retained completed predecessor")
                .state,
            "complete"
        );

        request.now_ms = 27;
        let replay = prepare_runner_volume_purge(&database.pool, &signer, &key_ring, &request)
            .expect("replay current successor attempt");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replay.status.request_id, successor.status.request_id);

        for volume in [&first_volume, &second_volume] {
            let ack = signed_ack(volume, command_for(&successor, volume), 26);
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 26)
                .expect("acknowledge successor target");
        }
        let successor_completion =
            complete_runner_volume_purge(&database.pool, &successor.status.request_id, 27)
                .expect("complete successor purge attempt");
        assert_eq!(successor_completion.status.state, "complete");
        let retained_tombstones: i64 = database
            .pool
            .get()
            .expect("open tombstone count connection")
            .query_row(
                "SELECT COUNT(*) FROM jobs_runner_purge_tombstones \
                  WHERE purge_subject = ?1",
                params![first.status.purge_subject],
                |row| row.get(0),
            )
            .expect("count retained successor tombstones");
        assert_eq!(retained_tombstones, 2);
    }

    #[test]
    fn runner_build_ids_are_canonical_and_monotonic() {
        for value in [
            "runner-0",
            "runner-602",
            "runner-602.0",
            "runner-999999999.1",
        ] {
            assert!(runner_build_id_is_canonical(value), "canonical {value}");
        }
        for value in [
            "runner-",
            "runner-01",
            "runner-602.01",
            "runner-602.",
            "runner-602.0.1",
            "runner-1000000000",
            "2026.08.0",
        ] {
            assert!(!runner_build_id_is_canonical(value), "malformed {value}");
        }
        assert!(runner_build_satisfies("runner-602", "runner-602.0"));
        assert!(runner_build_satisfies("runner-602.1", "runner-602"));
        assert!(runner_build_satisfies("runner-603", "runner-602.999"));
        assert!(!runner_build_satisfies("runner-602", "runner-602.1"));
        assert!(!runner_build_satisfies("runner-601.999", "runner-602"));
        assert!(!runner_build_satisfies("runner-602.01", "runner-602"));
    }

    #[test]
    fn purge_v2_command_requires_an_exact_legacy_authority_binding() {
        let signer = server_signer();
        let base = NewRunnerPurgeCommand {
            request_id: "request-authority-binding".to_string(),
            command_id: "command-authority-binding".to_string(),
            target_volume_id: encode_base64url(&[1_u8; 32]),
            target_key_fingerprint: sha256("target key"),
            enrollment_epoch: 1,
            purge_subject: encode_base64url(&[2_u8; 32]),
            purge_generation: 1,
            storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            legacy_inventory_authority_generation: 0,
            legacy_inventory_authority_sha256: ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256
                .to_string(),
            issued_at_ms: 1,
            minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
        };
        signer
            .sign_command(base.clone())
            .expect("sign canonical absent authority binding");

        let mut noncanonical_absent = base.clone();
        noncanonical_absent.legacy_inventory_authority_sha256 = sha256("not absent");
        assert!(matches!(
            signer.sign_command(noncanonical_absent),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));

        let mut generation_with_absent_digest = base.clone();
        generation_with_absent_digest.legacy_inventory_authority_generation = 1;
        assert!(matches!(
            signer.sign_command(generation_with_absent_digest),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));

        let mut established = base;
        established.legacy_inventory_authority_generation = 7;
        established.legacy_inventory_authority_sha256 = sha256("established authority");
        let command = signer
            .sign_command(established)
            .expect("sign established authority binding");
        let mut legacy_command = command.clone();
        legacy_command.version = 1;
        assert!(matches!(
            signer.verify_command(&legacy_command),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));
        let mut authority_tamper = command.clone();
        authority_tamper.legacy_inventory_authority_generation = 8;
        assert!(matches!(
            signer.verify_command(&authority_tamper),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
        let mut json = serde_json::to_value(command).expect("encode strict command");
        json.as_object_mut()
            .expect("command object")
            .insert("legacyAuthorityId".to_string(), json!("unfrozen-alias"));
        assert!(serde_json::from_value::<RunnerPurgeCommand>(json).is_err());
    }

    #[test]
    fn purge_v2_storage_evidence_is_closed_and_requires_canonical_zero_after_state() {
        let evidence = mixed_storage_evidence("strict-v2");
        evidence
            .validate()
            .expect("validate complete mixed evidence");
        assert_eq!(
            evidence
                .target_inventory_state(true)
                .expect("derive combined target")
                .0,
            5
        );
        assert_eq!(
            evidence
                .target_inventory_state(false)
                .expect("derive empty combined target"),
            (0, EMPTY_RUNNER_INVENTORY_SHA256.to_string())
        );

        let mut retained_locators = test_storage_evidence("retained-locators", 1);
        retained_locators.locators.count = 3;
        retained_locators.locators.sha256 = sha256("retained locator set");
        retained_locators
            .validate()
            .expect("allow a clean repurge with indefinitely retained locators");

        let mut locator_mismatch = evidence.clone();
        locator_mismatch.locators.count = 2;
        assert!(locator_mismatch.validate().is_err());

        let mut noncanonical_bytes = evidence.clone();
        noncanonical_bytes.root.before.file_bytes = "030".to_string();
        assert!(noncanonical_bytes.validate().is_err());

        let mut incomplete_root = evidence.clone();
        incomplete_root.root.before.entry_count = 6;
        assert!(incomplete_root.validate().is_err());

        let mut resident_after = evidence.clone();
        resident_after.subject_storage.after.residency = "resident".to_string();
        assert!(resident_after.validate().is_err());

        let mut legacy_after = evidence.clone();
        legacy_after.legacy.root_after.unclassified_root_count = 1;
        assert!(legacy_after.validate().is_err());

        let mut encoded = serde_json::to_value(&evidence).expect("encode storage evidence");
        encoded["legacy"]["rootAfter"]
            .as_object_mut()
            .expect("legacy after object")
            .insert(
                "legacyInventorySha256".to_string(),
                json!(sha256("unbound alias")),
            );
        assert!(serde_json::from_value::<RunnerPurgeStorageEvidence>(encoded).is_err());
    }

    #[test]
    fn node_and_rust_storage_attestation_vector_matches() {
        let signing_key = Ed25519SigningKey::from_bytes(&[0x0b_u8; 32]);
        let public_key = encode_base64url(signing_key.verifying_key().as_bytes());
        let volume_id =
            runner_volume_id_from_public_key(&public_key).expect("derive vector volume");
        let volume_key_fingerprint =
            runner_volume_key_fingerprint(&public_key).expect("fingerprint vector key");
        let attestation = sign_runner_volume_storage_attestation(
            &signing_key,
            NewRunnerVolumeStorageAttestation {
                attestation_id: "attestation-vector-1".to_string(),
                volume_id,
                volume_key_fingerprint,
                resource_fingerprint: sha256("resource-vector"),
                enrollment_epoch: 1,
                enrollment_generation: 7,
                process_instance_id: encode_base64url(&[0x0c_u8; 32]),
                predecessor_attestation_generation: 3,
                predecessor_attestation_sha256: sha256("previous-attestation"),
                required_tombstone_generation: 5,
                reconciled_tombstone_generation: 5,
                storage_evidence_version: 2,
                subject_storage_layout_version: 2,
                root_device_id: "device-vector-1".to_string(),
                root_link_count: 7,
                root_entry_count: 29,
                root_file_bytes: "4096".to_string(),
                root_sha256: sha256("root-vector"),
                subject_storage_subject_count: 2,
                subject_storage_subject_set_sha256: sha256("subjects-vector"),
                subject_storage_scope_count: 3,
                subject_storage_complete_root_entry_count: 18,
                subject_storage_complete_root_file_bytes: "3072".to_string(),
                subject_storage_complete_root_sha256: sha256("account-data-vector"),
                locator_count: 5,
                resident_locator_count: 3,
                locator_set_sha256: sha256("locators-vector"),
                legacy_inventory_version: 1,
                legacy_artifact_count: 0,
                legacy_artifact_bytes: "0".to_string(),
                legacy_artifact_set_sha256: EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.to_string(),
                unclassified_root_count: 0,
                runner_build_id: "runner-602.1".to_string(),
                observed_at_ms: 1_750_000_000_000,
            },
        )
        .expect("sign storage attestation vector");
        assert_eq!(
            attestation
                .canonical_unsigned_sha256()
                .expect("hash unsigned vector"),
            "6dfe24fee1e8364a314dd3c82e21326f5ff8f7d52f982678bcfa1525291ab2cf"
        );
        assert_eq!(
            attestation.signature,
            "1oGMUm4-B6Og2DXENeDTT33CofW2QhsRHcDhesVSxLB-43PJ5f7ymIcGnb0U-4-wxde6PJRNndWqwWmprnqzBw"
        );
        assert_eq!(
            attestation
                .attestation_sha256()
                .expect("hash signed vector"),
            "cbc3eabe0fe038531082be1341e693080f1e9d7ca192c7e8157ff73be099064d"
        );
        verify_runner_volume_storage_attestation(&attestation, &public_key)
            .expect("verify storage attestation vector");
    }

    #[test]
    fn storage_attestation_is_append_only_exact_and_freshly_authorized() {
        let database = TestDatabase::new(&[]);
        let volume = fixed_volume(&database.pool, 70, "storage-ledger", 1);
        claim_runner_volume_instance(
            &database.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            10,
            900_000,
        )
        .expect("claim storage-ledger volume");

        let first = signed_fixed_storage_attestation(&database.pool, &volume, 11);
        let first_sha256 = first.attestation_sha256().expect("hash first attestation");
        let first_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &first,
            "storage-ledger-applied",
            11,
        );
        let applied = record_runner_volume_storage_attestation(
            &database.pool,
            &first,
            TEST_RUNNER_BUILD,
            11,
            &first_authority,
        )
        .expect("append first storage attestation");
        assert_eq!(applied.disposition, RunnerVolumeWriteDisposition::Applied);
        assert_eq!(applied.attestation_generation, 1);
        assert_eq!(applied.fleet_attestation_generation, 1);
        assert_eq!(applied.attestation_sha256, first_sha256);
        assert_eq!(applied.volume_status, "active");

        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &first,
                TEST_RUNNER_BUILD,
                12,
                &first_authority,
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
        let replay_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &first,
            "storage-ledger-replay",
            12,
        );
        let replayed = record_runner_volume_storage_attestation(
            &database.pool,
            &first,
            TEST_RUNNER_BUILD,
            12,
            &replay_authority,
        )
        .expect("replay exact storage attestation with fresh authority");
        assert_eq!(replayed.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(
            replayed,
            RunnerVolumeStorageAttestationOutcome {
                disposition: RunnerVolumeWriteDisposition::Replay,
                ..applied.clone()
            }
        );
        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &first,
                TEST_RUNNER_BUILD,
                13,
                &replay_authority,
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));

        let conflicting = resign_fixed_storage_attestation(&volume, first.clone(), |attestation| {
            attestation.observed_at_ms = 12
        });
        let conflicting_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &conflicting,
            "storage-ledger-conflict",
            13,
        );
        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &conflicting,
                TEST_RUNNER_BUILD,
                13,
                &conflicting_authority,
            ),
            Err(RunnerVolumePurgeError::Conflict)
        ));

        let second = signed_fixed_storage_attestation(&database.pool, &volume, 14);
        let stale_second =
            resign_fixed_storage_attestation(&volume, second.clone(), |attestation| {
                attestation.attestation_id = "storage-attestation-stale-predecessor".to_string();
                attestation.observed_at_ms = 15;
            });
        let second_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &second,
            "storage-ledger-second",
            14,
        );
        let second_outcome = record_runner_volume_storage_attestation(
            &database.pool,
            &second,
            TEST_RUNNER_BUILD,
            14,
            &second_authority,
        )
        .expect("append second storage attestation");
        assert_eq!(second_outcome.attestation_generation, 2);
        let stale_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &stale_second,
            "storage-ledger-stale-predecessor",
            15,
        );
        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &stale_second,
                TEST_RUNNER_BUILD,
                15,
                &stale_authority,
            ),
            Err(RunnerVolumePurgeError::Conflict)
        ));

        let future = resign_fixed_storage_attestation(
            &volume,
            signed_fixed_storage_attestation(&database.pool, &volume, 16),
            |attestation| {
                attestation.attestation_id = "storage-attestation-future".to_string();
                attestation.observed_at_ms = 300_017;
            },
        );
        let future_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &future,
            "storage-ledger-future",
            16,
        );
        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &future,
                TEST_RUNNER_BUILD,
                16,
                &future_authority,
            ),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));

        let stale_process_attestation =
            signed_fixed_storage_attestation(&database.pool, &volume, 17);
        let stale_process_authority = fixed_storage_attestation_authority(
            &database.pool,
            &volume,
            &stale_process_attestation,
            "storage-ledger-stale-process",
            17,
        );
        let replacement_process = encode_base64url(&[71_u8; 32]);
        claim_runner_volume_instance(
            &database.pool,
            &volume.volume_id,
            1,
            &replacement_process,
            900_001,
            1_000_000,
        )
        .expect("replace expired storage attestation process");
        assert!(matches!(
            record_runner_volume_storage_attestation(
                &database.pool,
                &stale_process_attestation,
                TEST_RUNNER_BUILD,
                900_002,
                &stale_process_authority,
            ),
            Err(RunnerVolumePurgeError::NotReady)
        ));

        let conn = database.pool.get().expect("open storage ledger connection");
        let stored: (i64, String, String, String) = conn
            .query_row(
                "SELECT COUNT(*), MIN(canonical_unsigned_sha256), \
                        MIN(attestation_sha256), MIN(canonical_json) \
                   FROM jobs_runner_volume_storage_attestations \
                  WHERE volume_id = ?1",
                params![volume.volume_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("load immutable storage ledger rows");
        assert_eq!(stored.0, 2);
        assert_eq!(stored.1.len(), 64);
        assert_eq!(stored.2.len(), 64);
        assert!(stored.3.contains("\"attestationId\""));
        assert!(conn
            .execute(
                "UPDATE jobs_runner_volume_storage_attestations \
                    SET runner_build_id = 'runner-999' WHERE volume_id = ?1",
                params![volume.volume_id],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM jobs_runner_volume_storage_attestations WHERE volume_id = ?1",
                params![volume.volume_id],
            )
            .is_err());
    }

    #[test]
    fn storage_attestation_reconciles_legacy_enrollment_and_preserves_admin_state() {
        let database = TestDatabase::new(&[]);
        let legacy_volume =
            fixed_volume_with_legacy_count(&database.pool, 72, "legacy-attestation", 1, 9);
        claim_runner_volume_instance(
            &database.pool,
            &legacy_volume.volume_id,
            1,
            &legacy_volume.process_instance_id,
            10,
            900_000,
        )
        .expect("claim legacy-enrollment volume");
        let legacy_before = lookup_runner_volume(&database.pool, &legacy_volume.volume_id)
            .expect("load legacy-enrollment volume")
            .expect("legacy-enrollment volume exists");
        assert_eq!(legacy_before.legacy_artifact_count, 9);
        let legacy_outcome = attest_fixed_volume(&database.pool, &legacy_volume, 11);
        assert_eq!(legacy_outcome.volume_status, "active");
        let stored_legacy_count: i64 = database
            .pool
            .get()
            .expect("open legacy evidence connection")
            .query_row(
                "SELECT legacy_artifact_count \
                   FROM jobs_runner_volume_storage_attestations WHERE volume_id = ?1",
                params![legacy_volume.volume_id],
                |row| row.get(0),
            )
            .expect("load current zero legacy evidence");
        assert_eq!(stored_legacy_count, 0);

        for (seed, label, status, at_ms) in [
            (73_u8, "attestation-suspended", "suspended", 20_i64),
            (74_u8, "attestation-retired", "retired", 30_i64),
        ] {
            let volume = fixed_volume(&database.pool, seed, label, at_ms);
            claim_runner_volume_instance(
                &database.pool,
                &volume.volume_id,
                1,
                &volume.process_instance_id,
                at_ms + 1,
                at_ms + 900_000,
            )
            .expect("claim administratively managed volume");
            database
                .pool
                .get()
                .expect("open administrative state connection")
                .execute(
                    "UPDATE jobs_runner_volumes SET status = ?2 WHERE volume_id = ?1",
                    params![volume.volume_id, status],
                )
                .expect("set administrative volume state");
            let outcome = attest_fixed_volume(&database.pool, &volume, at_ms + 2);
            assert_eq!(outcome.volume_status, status);
            assert_eq!(
                lookup_runner_volume(&database.pool, &volume.volume_id)
                    .expect("reload administratively managed volume")
                    .expect("administratively managed volume exists")
                    .status,
                status
            );
        }
    }

    #[test]
    fn enrollment_after_completed_purge_reopens_inventory_and_blocks_hard_delete() {
        let database = TestDatabase::new(&["acct-enrollment-race"]);
        let signer = server_signer();
        let key_ring = server_key_ring(&signer);
        let original = fixed_volume(&database.pool, 75, "enrollment-race-original", 1);
        claim_fixed_volume(&database.pool, &original, 2);
        let prepared = prepare(
            &database,
            &signer,
            "acct-enrollment-race",
            "request-enrollment-race",
            10,
            75,
        );
        let ack = signed_ack(&original, command_for(&prepared, &original), 11);
        acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 11)
            .expect("acknowledge enrollment-race purge");
        let completed =
            complete_runner_volume_purge(&database.pool, &prepared.status.request_id, 12)
                .expect("complete enrollment-race purge");
        assert_eq!(completed.status.state, "complete");

        let newcomer = fixed_volume(&database.pool, 76, "enrollment-race-new", 13);
        let fleet =
            runner_volume_fleet_status(&database.pool).expect("load fleet after racing enrollment");
        assert_eq!(fleet.legacy_inventory_state, "reconciling");
        assert!(lookup_runner_volume(&database.pool, &newcomer.volume_id)
            .expect("load racing enrollment")
            .expect("racing enrollment exists")
            .active_instance_id
            .is_none());
        let missing_workflow_cleanup = JobsWorkflowCleanupDeletionProof {
            account_generation: 10,
            cleanup_generation_id: "wfcleanupgen-v3-missing-test".to_string(),
            target_set_digest: "7".repeat(64),
            legacy_authority: JobsLegacyInventoryAuthorityRef {
                inventory_generation_id: "wfinventory-v3-missing-test".to_string(),
                query_digest: "8".repeat(64),
            },
            tombstone_id: "wfcleantomb-v3-missing-test".to_string(),
            completion_digest: "9".repeat(64),
        };
        assert!(
            crate::db::account_data::hard_delete_account_after_runner_purge(
                &database.pool,
                "acct-enrollment-race",
                10,
                &prepared.status.request_id,
                "wfsweep-account-v3-missing-test",
                &missing_workflow_cleanup,
            )
            .is_err()
        );
        assert_eq!(
            runner_volume_purge_status(&database.pool, &prepared.status.request_id)
                .expect("retain completed frozen request")
                .required_target_count,
            1
        );
    }

    #[test]
    fn node_and_rust_command_ack_and_authority_vectors_match() {
        let signer = server_signer();
        let volume_key = Ed25519SigningKey::from_bytes(&[9_u8; 32]);
        let volume_public = encode_base64url(volume_key.verifying_key().as_bytes());
        let volume_id = runner_volume_id_from_public_key(&volume_public).expect("derive volume id");
        let fingerprint = runner_volume_key_fingerprint(&volume_public).expect("fingerprint key");
        assert_eq!(volume_id, "gpZyp68F-efxcxgBNDDQrX4XU_SauTNYymo-vF81hFM");
        assert_eq!(
            fingerprint,
            "dbc298251c51321b7266e78d1c151c2b62aff8cb95b293096d3463018544face"
        );
        let command = signer
            .sign_command(NewRunnerPurgeCommand {
                request_id: "request-vector".to_string(),
                command_id: "command-vector".to_string(),
                target_volume_id: volume_id.clone(),
                target_key_fingerprint: fingerprint.clone(),
                enrollment_epoch: 1,
                purge_subject: "CwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCws".to_string(),
                purge_generation: 3,
                storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
                subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
                legacy_inventory_authority_generation: 0,
                legacy_inventory_authority_sha256: ABSENT_RUNNER_LEGACY_INVENTORY_AUTHORITY_SHA256
                    .to_string(),
                issued_at_ms: 1_700_000_000_123,
                minimum_runner_build_id: "runner-602.1".to_string(),
            })
            .expect("sign vector command");
        assert_eq!(
            command.signature,
            "i_jumYN0vtasqIfC_W8M2UFktSCL2iUWjkMNy51uEEdH4VqZTBiRyECfxlcvMDGvUsZoRchikJP42sadpT38Dw"
        );
        assert_eq!(
            command.command_sha256().expect("hash vector command"),
            "5b48a3aa0e148f56d9743ae44c682ebba6555aa3071b5bdf74c9fdf7054a676a"
        );
        signer
            .verify_command(&command)
            .expect("verify vector command");
        let storage_evidence = test_storage_evidence("vector", 2);
        let storage_evidence_sha256 = storage_evidence
            .sha256()
            .expect("hash vector storage evidence");
        assert_eq!(
            storage_evidence_sha256,
            "7b8ee0312d1fc0431f9e754b66f547639530a4c4445457702b87c76928d07c28"
        );
        let before_inventory = storage_evidence
            .target_inventory_state(true)
            .expect("derive vector before inventory");
        assert_eq!(
            before_inventory,
            (
                2,
                "51a1d64fc895d483e235f63cf2216214b1c47476f49cbdd237773433034da311".to_string(),
            )
        );
        let after_inventory = storage_evidence
            .target_inventory_state(false)
            .expect("derive vector after inventory");
        let ack = sign_runner_purge_ack(
            &volume_key,
            NewRunnerPurgeAck {
                request_id: command.request_id.clone(),
                command_id: command.command_id.clone(),
                command_sha256: command.command_sha256().expect("hash vector command"),
                target_volume_id: volume_id.clone(),
                target_key_fingerprint: fingerprint,
                enrollment_epoch: 1,
                process_instance_id: "DAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAw".to_string(),
                purge_subject_sha256:
                    "f0e38b830ebd8a506615ecd154330ec07ff6bf5030447b44e297db1d4b7514ac".to_string(),
                purge_generation: 3,
                storage_evidence,
                storage_evidence_sha256,
                before_inventory_count: before_inventory.0,
                before_inventory_sha256: before_inventory.1,
                after_inventory_count: after_inventory.0,
                after_inventory_sha256: after_inventory.1,
                removed_count: before_inventory.0 - after_inventory.0,
                runner_build_id: "runner-602.1".to_string(),
                completed_at_ms: 1_700_000_000_999,
            },
        )
        .expect("sign vector acknowledgement");
        assert_eq!(
            ack.signature,
            "xufw1DQq7NgpaRQ_OZ_jq9VXN3ROvUybdHdzX7rVpaHs-cv42csqV8gBx0xMAWYGTd6w41H7b1VTU26mxmuqCA"
        );
        assert_eq!(
            ack.ack_sha256().expect("hash vector acknowledgement"),
            "4696abc55eaea600b70a7b4a023376f7ed801f3233968f86e182854462beff30"
        );
        verify_runner_purge_ack(&ack, &volume_public).expect("verify vector acknowledgement");
        let authority = sign_runner_volume_authority_proof(
            &volume_key,
            NewRunnerVolumeAuthorityProof {
                operation: "purge_poll".to_string(),
                request_id: "authority-vector".to_string(),
                volume_id,
                enrollment_epoch: 1,
                process_instance_id: ack.process_instance_id.clone(),
                issued_at_ms: 1_700_000_000_500,
                payload_sha256: "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5"
                    .to_string(),
            },
        )
        .expect("sign authority vector");
        assert_eq!(
            authority.signature,
            "F4L_VTau5buZhgdZC0LQ_ZqH_yvEPx_MMHKxHYEvGsr5XI047D0PXJ9XtRZxqjH4_EZkztHVm1L84NMR1FxtBA"
        );
        verify_runner_volume_authority_signature(&authority, &volume_public)
            .expect("verify authority vector");
        let mut legacy_ack = ack.clone();
        legacy_ack.version = 1;
        assert!(matches!(
            verify_runner_purge_ack(&legacy_ack, &volume_public),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));
        let mut ack_with_unknown_field =
            serde_json::to_value(&ack).expect("encode strict vector acknowledgement");
        ack_with_unknown_field
            .as_object_mut()
            .expect("acknowledgement object")
            .insert(
                "inventorySha256".to_string(),
                json!(sha256("ambiguous alias")),
            );
        assert!(serde_json::from_value::<RunnerPurgeAck>(ack_with_unknown_field).is_err());
        let mut forged_ack = ack;
        forged_ack.storage_evidence.root.before.sha256 = sha256("forged root inventory");
        forged_ack.storage_evidence_sha256 = forged_ack
            .storage_evidence
            .sha256()
            .expect("hash forged vector evidence");
        assert!(matches!(
            verify_runner_purge_ack(&forged_ack, &volume_public),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
    }

    #[test]
    fn enrollment_is_replay_safe_and_live_clone_claim_is_fenced() {
        let database = TestDatabase::new(&[]);
        let volume = fixed_volume(&database.pool, 19, "clone", 10);
        claim_fixed_volume(&database.pool, &volume, 11);
        let clone_instance = encode_base64url(&[201_u8; 32]);
        assert!(matches!(
            claim_runner_volume_instance(
                &database.pool,
                &volume.volume_id,
                1,
                &clone_instance,
                12,
                900_000,
            ),
            Err(RunnerVolumePurgeError::Conflict)
        ));
        let replaced = claim_runner_volume_instance(
            &database.pool,
            &volume.volume_id,
            1,
            &clone_instance,
            900_001,
            950_000,
        )
        .expect("replace expired process instance");
        assert_eq!(replaced.disposition, RunnerVolumeWriteDisposition::Applied);
    }

    #[test]
    fn persisted_commands_require_trusted_historical_key_after_rotation() {
        let database = TestDatabase::new(&["acct-rotation"]);
        let old_signer = server_signer();
        let rotation_volume = fixed_volume(&database.pool, 20, "rotation", 10);
        claim_fixed_volume(&database.pool, &rotation_volume, 11);
        let prepared = prepare(
            &database,
            &old_signer,
            "acct-rotation",
            "request-rotation",
            20,
            31,
        );
        let new_signer = RunnerPurgeSigner::from_seed("server-key-2", [8_u8; 32])
            .expect("construct rotated signer");
        let current_only_ring = server_key_ring(&new_signer);
        let replay_input = PrepareRunnerVolumePurgeRequest {
            request_id: "request-rotation".to_string(),
            account_id: "acct-rotation".to_string(),
            minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
            expected_legacy_inventory_generation: prepared.status.legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: prepared
                .status
                .legacy_inventory_reconciliation_id
                .clone(),
            expected_legacy_inventory_authority_id: prepared
                .status
                .legacy_inventory_authority_id
                .clone(),
            expected_legacy_inventory_authority_sha256: prepared
                .status
                .legacy_inventory_authority_sha256
                .clone(),
            now_ms: 21,
        };
        assert!(matches!(
            prepare_runner_volume_purge_with_subject_material(
                &database.pool,
                &new_signer,
                &current_only_ring,
                &replay_input,
                [32_u8; 32],
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
        let rotated_ring = RunnerPurgeCommandKeyRing::new(
            &new_signer,
            BTreeMap::from([
                (
                    old_signer.key_id().to_string(),
                    old_signer.public_key_base64url(),
                ),
                (
                    new_signer.key_id().to_string(),
                    new_signer.public_key_base64url(),
                ),
            ]),
        )
        .expect("construct rotated verification ring");
        let replay = prepare_runner_volume_purge_with_subject_material(
            &database.pool,
            &new_signer,
            &rotated_ring,
            &replay_input,
            [33_u8; 32],
        )
        .expect("verify historical command with retired key");
        assert_eq!(replay.commands, prepared.commands);

        let mut forged = prepared.commands[0].clone();
        forged.signature = encode_base64url(&[0_u8; 64]);
        let forged_json = serde_json::to_string(&forged).expect("serialize forged command");
        database
            .pool
            .get()
            .expect("open test connection")
            .execute(
                "UPDATE jobs_runner_purge_targets \
                    SET command_json = ?2, server_signature = ?3 \
                  WHERE request_id = ?1",
                params!["request-rotation", forged_json, forged.signature],
            )
            .expect("tamper persisted command signature");
        assert!(matches!(
            prepare_runner_volume_purge_with_subject_material(
                &database.pool,
                &new_signer,
                &rotated_ring,
                &replay_input,
                [34_u8; 32],
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));
    }

    #[test]
    fn signed_volume_authority_request_ids_are_consumed_once_per_epoch() {
        let database = TestDatabase::new(&[]);
        let volume = fixed_volume(&database.pool, 30, "authority-replay", 10);
        let runtime = RunnerProcessRuntimeAttestation {
            runner_image_sha256: sha256("authority-runtime-image"),
            runner_build_id: TEST_RUNNER_BUILD.to_string(),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: sha256("authority-runtime-bundle"),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "123456".to_string(),
            chromium_executable_sha256: sha256("authority-runtime-chromium"),
        };
        let runtime_token = encode_base64url(&[91_u8; 32]);
        create_runner_process_runtime_grant(
            &database.pool,
            &NewRunnerProcessRuntimeGrant {
                grant_id: "runtime-grant-authority-replay".to_string(),
                token: runtime_token.clone(),
                expected_worker_id: "worker-authority-replay".to_string(),
                runtime: runtime.clone(),
                authorization_ref: "deployment-authority-replay".to_string(),
                created_by: "test-operator".to_string(),
                expires_at_ms: 1_000,
                created_at_ms: 10,
            },
        )
        .expect("create process runtime grant");
        let runtime_claim = RunnerProcessRuntimeGrantClaim {
            grant_id: "runtime-grant-authority-replay".to_string(),
            grant_token: runtime_token,
            runtime,
        };
        let claim_payload_sha256 = sha256(b"authority-claim-payload");
        let claim_proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "instance_claim".to_string(),
                request_id: "authority-request-replay".to_string(),
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                issued_at_ms: 11,
                payload_sha256: claim_payload_sha256.clone(),
            },
        )
        .expect("sign instance claim authority");
        let claim_authority = verify_runner_volume_authority_proof(
            &database.pool,
            &claim_proof,
            "instance_claim",
            &claim_payload_sha256,
            11,
            90_000,
        )
        .expect("verify instance claim authority");
        claim_runner_volume_instance_authorized(
            &database.pool,
            &RunnerVolumeInstanceLeaseRequest {
                volume_id: &volume.volume_id,
                enrollment_epoch: 1,
                process_instance_id: &volume.process_instance_id,
                now_ms: 11,
                lease_expires_at_ms: 100,
            },
            &runtime_claim,
            &claim_authority,
        )
        .expect("consume instance claim authority");
        assert!(matches!(
            claim_runner_volume_instance_authorized(
                &database.pool,
                &RunnerVolumeInstanceLeaseRequest {
                    volume_id: &volume.volume_id,
                    enrollment_epoch: 1,
                    process_instance_id: &volume.process_instance_id,
                    now_ms: 12,
                    lease_expires_at_ms: 101,
                },
                &runtime_claim,
                &claim_authority,
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));

        let heartbeat_payload_sha256 = sha256(b"authority-heartbeat-payload");
        let duplicate_request_proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "instance_heartbeat".to_string(),
                request_id: claim_proof.request_id,
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                issued_at_ms: 12,
                payload_sha256: heartbeat_payload_sha256.clone(),
            },
        )
        .expect("sign duplicate request-id heartbeat authority");
        let duplicate_request_authority = verify_runner_volume_authority_proof(
            &database.pool,
            &duplicate_request_proof,
            "instance_heartbeat",
            &heartbeat_payload_sha256,
            12,
            90_000,
        )
        .expect("verify duplicate request-id heartbeat authority");
        assert!(matches!(
            heartbeat_runner_volume_instance_authorized(
                &database.pool,
                &volume.volume_id,
                1,
                &volume.process_instance_id,
                12,
                101,
                &duplicate_request_authority,
            ),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));

        let fresh_heartbeat_proof = sign_runner_volume_authority_proof(
            &volume.signing_key,
            NewRunnerVolumeAuthorityProof {
                operation: "instance_heartbeat".to_string(),
                request_id: "authority-request-fresh".to_string(),
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                issued_at_ms: 12,
                payload_sha256: heartbeat_payload_sha256.clone(),
            },
        )
        .expect("sign fresh heartbeat authority");
        let fresh_heartbeat_authority = verify_runner_volume_authority_proof(
            &database.pool,
            &fresh_heartbeat_proof,
            "instance_heartbeat",
            &heartbeat_payload_sha256,
            12,
            90_000,
        )
        .expect("verify fresh heartbeat authority");
        heartbeat_runner_volume_instance_authorized(
            &database.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            12,
            101,
            &fresh_heartbeat_authority,
        )
        .expect("consume fresh heartbeat authority");
        let use_count: i64 = database
            .pool
            .get()
            .expect("open authority-use verification connection")
            .query_row(
                "SELECT COUNT(*) FROM jobs_runner_volume_authority_uses",
                [],
                |row| row.get(0),
            )
            .expect("count consumed authority requests");
        assert_eq!(use_count, 2);
    }

    #[test]
    fn cutover_ready_is_bound_to_exact_snapshot_and_enrollment_invalidates_it() {
        let database = TestDatabase::new(&[]);
        let signer = server_signer();
        let key_ring = server_key_ring(&signer);
        let volume = fixed_volume(&database.pool, 25, "cutover-active", 10);
        claim_fixed_volume(&database.pool, &volume, 11);
        assert!(poll_runner_volume_purge_commands(
            &database.pool,
            &signer,
            &key_ring,
            &PollRunnerVolumePurgeCommandsRequest {
                volume_id: volume.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume.process_instance_id.clone(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                now_ms: 12,
                limit: 10,
                after_command_id: None,
            },
        )
        .expect("poll reconciled enrolled volume")
        .commands
        .is_empty());
        assert_eq!(
            activate_reconciled_runner_volume(
                &database.pool,
                &volume.volume_id,
                1,
                &volume.process_instance_id,
                13,
            )
            .expect("activate volume-local readiness before fleet cutover"),
            RunnerVolumeWriteDisposition::Replay
        );
        ensure_legacy_inventory_ready(&database.pool, "cutover-active", 12);
        let before = runner_volume_fleet_status(&database.pool).expect("load active fleet");
        assert_eq!(before.cutover_state, "pre_cutover");
        assert_eq!(before.enrollment_generation, 1);
        assert_eq!(before.non_destroyed_volume_count, 1);
        assert_eq!(before.attested_reconciled_volume_count, 1);
        let mut request = RecordRunnerVolumeFleetCutoverRequest {
            cutover_state: "reconciling".to_string(),
            expected_enrollment_generation: before.enrollment_generation,
            expected_purge_generation: before.purge_generation,
            expected_tombstone_generation: before.tombstone_generation,
            expected_destruction_generation: before.destruction_generation,
            expected_legacy_reconciliation_generation: before.legacy_reconciliation_generation,
            expected_storage_attestation_generation: before.storage_attestation_generation,
            expected_storage_attestation_count: before.storage_attestation_count,
            expected_storage_attestation_set_sha256: before.storage_attestation_set_sha256.clone(),
            expected_legacy_inventory_generation: before.legacy_inventory_generation,
            expected_legacy_inventory_reconciliation_id: before
                .legacy_inventory_reconciliation_id
                .clone()
                .expect("legacy inventory reconciliation id"),
            expected_legacy_inventory_authority_id: before
                .legacy_inventory_authority_id
                .clone()
                .expect("legacy inventory authority id"),
            expected_legacy_inventory_authority_sha256: before
                .legacy_inventory_authority_sha256
                .clone()
                .expect("legacy inventory authority digest"),
            expected_legacy_inventory_root_count: before
                .legacy_inventory_root_count
                .expect("legacy inventory root count"),
            expected_legacy_inventory_root_set_sha256: before
                .legacy_inventory_root_set_sha256
                .clone()
                .expect("legacy inventory root-set digest"),
            expected_non_destroyed_volume_count: before.non_destroyed_volume_count,
            expected_destruction_count: before.destruction_count,
            expected_unresolved_legacy_volume_count: before.unresolved_legacy_volume_count,
            evidence_ref: "cutover-evidence-ref".to_string(),
            evidence_sha256: sha256(b"cutover-evidence"),
            authorized_by: "test-operator".to_string(),
            cutover_at_ms: 14,
            now_ms: 14,
        };
        let mut direct_ready = request.clone();
        direct_ready.cutover_state = "ready".to_string();
        assert!(matches!(
            record_runner_volume_fleet_cutover(&database.pool, &direct_ready),
            Err(RunnerVolumePurgeError::NotReady)
        ));
        assert_eq!(
            record_runner_volume_fleet_cutover(&database.pool, &request)
                .expect("freeze cutover snapshot"),
            RunnerVolumeWriteDisposition::Applied
        );
        let frozen = runner_volume_fleet_status(&database.pool).expect("load frozen fleet");
        assert_eq!(frozen.cutover_enrollment_generation, Some(1));
        assert_eq!(frozen.cutover_non_destroyed_volume_count, Some(1));
        let mut concurrent_exact_replay = request.clone();
        concurrent_exact_replay.cutover_at_ms = 15;
        concurrent_exact_replay.now_ms = 15;
        assert_eq!(
            record_runner_volume_fleet_cutover(&database.pool, &concurrent_exact_replay)
                .expect("replay exact cutover despite independently minted server time"),
            RunnerVolumeWriteDisposition::Replay
        );
        assert_eq!(
            runner_volume_fleet_status(&database.pool)
                .expect("load replayed cutover")
                .cutover_at_ms,
            Some(14)
        );
        request.cutover_state = "ready".to_string();
        request.cutover_at_ms = 16;
        request.now_ms = 16;
        assert_eq!(
            record_runner_volume_fleet_cutover(&database.pool, &request)
                .expect("mark exact snapshot ready"),
            RunnerVolumeWriteDisposition::Applied
        );
        assert_eq!(
            runner_volume_fleet_status(&database.pool)
                .expect("load ready fleet")
                .cutover_state,
            "ready"
        );

        fixed_volume(&database.pool, 26, "cutover-invalidation", 20);
        let invalidated =
            runner_volume_fleet_status(&database.pool).expect("load invalidated fleet");
        assert_eq!(invalidated.cutover_state, "reconciling");
        assert_eq!(invalidated.enrollment_generation, 2);
        assert!(invalidated.cutover_enrollment_generation.is_none());
        assert!(invalidated.cutover_evidence_ref.is_none());
        assert!(matches!(
            record_runner_volume_fleet_cutover(&database.pool, &request),
            Err(RunnerVolumePurgeError::Conflict)
        ));
    }

    #[test]
    fn runner_volume_execution_lease_claim_is_atomic_with_residency_binding() {
        let success = TestDatabase::new(&["acct-claim-success"]);
        let (application_id, run_id, browser_profile_id) = runner_execution_lease_fixture(
            &success.pool,
            "acct-claim-success",
            "runner-purge-0@example.test",
            "runner-volume-success",
        );
        let volume = fixed_volume(&success.pool, 27, "claim-success", 10);
        let claim_now = now_ms();
        claim_runner_volume_instance(
            &success.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            claim_now,
            claim_now.saturating_add(5_000),
        )
        .expect("claim runner process for execution lease");
        attest_fixed_volume(&success.pool, &volume, claim_now.saturating_add(1));
        activate_reconciled_runner_volume(
            &success.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            claim_now.saturating_add(1),
        )
        .expect("activate runner volume before fleet readiness");
        mark_current_fleet_ready(&success.pool, "claim-success", claim_now.saturating_add(2));
        let binding_request = BindRunnerVolumeResidencyRequest {
            account_id: "acct-claim-success".to_string(),
            run_id: run_id.clone(),
            worker_id: "worker-claim-success".to_string(),
            volume_id: volume.volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: volume.process_instance_id.clone(),
            now_ms: claim_now.saturating_add(4),
        };
        let grant = claim_execution_lease_for_runner_volume(
            &success.pool,
            "acct-claim-success",
            &application_id,
            &run_id,
            &browser_profile_id,
            "worker-claim-success",
            &binding_request,
        )
        .expect("claim execution lease with atomic runner binding");
        assert_eq!(grant.lease.run_id, run_id);
        assert_eq!(grant.volume_id, volume.volume_id);
        assert_eq!(grant.enrollment_epoch, 1);
        assert_eq!(grant.process_instance_id, volume.process_instance_id);
        assert_eq!(grant.volume_key_fingerprint, volume.key_fingerprint);
        assert_eq!(
            runner_purge_subject_sha256(&grant.purge_subject).expect("hash grant subject"),
            success
                .pool
                .get()
                .expect("open claim verification connection")
                .query_row(
                    "SELECT purge_subject_sha256 FROM jobs_execution_lease_volume_bindings \
                      WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .expect("load exact lease binding hash")
        );
        let mut conn = success
            .pool
            .get()
            .expect("open current binding transaction");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin current binding transaction");
        require_current_runner_volume_lease_binding_sqlite_tx(
            &tx,
            "acct-claim-success",
            &run_id,
            binding_request.now_ms.saturating_add(1),
        )
        .expect("authorize current runner lease binding");
        let durable_counts: (i64, i64, i64, i64) = tx
            .query_row(
                "SELECT \
                    (SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1), \
                    (SELECT COUNT(*) FROM jobs_execution_lease_volume_bindings \
                      WHERE run_id = ?1), \
                    (SELECT COUNT(*) FROM jobs_runner_account_subjects \
                      WHERE account_id = 'acct-claim-success'), \
                    (SELECT COUNT(*) FROM jobs_runner_volume_residencies \
                      WHERE purge_subject = ?2 AND state = 'resident' AND purge_generation = 0)",
                params![run_id, grant.purge_subject],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("load durable atomic claim rows");
        assert_eq!(durable_counts, (1, 1, 1, 1));
        tx.commit().expect("commit current binding verification");

        let replacement_process_instance_id = encode_base64url(&[201; 32]);
        assert!(matches!(
            claim_runner_volume_instance(
                &success.pool,
                &volume.volume_id,
                1,
                &replacement_process_instance_id,
                claim_now.saturating_add(4_999),
                claim_now.saturating_add(300_000),
            ),
            Err(RunnerVolumePurgeError::Conflict)
        ));
        let recovery_now = claim_now.saturating_add(5_001);
        let recovered_instance = claim_runner_volume_instance(
            &success.pool,
            &volume.volume_id,
            1,
            &replacement_process_instance_id,
            recovery_now,
            recovery_now.saturating_add(300_000),
        )
        .expect("replace the expired runner process on the same volume");
        assert_eq!(
            recovered_instance.disposition,
            RunnerVolumeWriteDisposition::Applied
        );
        let mut recovered_volume = volume.clone();
        recovered_volume.process_instance_id = replacement_process_instance_id.clone();
        attest_fixed_volume(&success.pool, &recovered_volume, recovery_now);
        mark_current_fleet_ready(
            &success.pool,
            "claim-recovery",
            recovery_now.saturating_add(1),
        );
        let recovery_binding_request = BindRunnerVolumeResidencyRequest {
            process_instance_id: replacement_process_instance_id.clone(),
            now_ms: recovery_now.saturating_add(3),
            ..binding_request.clone()
        };
        let recovered_grant = claim_execution_lease_for_runner_volume(
            &success.pool,
            "acct-claim-success",
            &application_id,
            &run_id,
            &browser_profile_id,
            "worker-claim-success",
            &recovery_binding_request,
        )
        .expect("recover the execution lease on the same volume and subject");
        assert!(recovered_grant.lease.fence > grant.lease.fence);
        assert_eq!(recovered_grant.purge_subject, grant.purge_subject);
        assert_eq!(
            recovered_grant.process_instance_id,
            replacement_process_instance_id
        );
        let recovered_binding: (String, String, i64) = success
            .pool
            .get()
            .expect("open recovery verification connection")
            .query_row(
                "SELECT process_instance_id, purge_subject_sha256, \
                        (SELECT COUNT(*) FROM jobs_runner_volume_residencies \
                          WHERE purge_subject = ?2 AND volume_id = ?3 AND volume_epoch = 1) \
                   FROM jobs_execution_lease_volume_bindings WHERE run_id = ?1",
                params![run_id, grant.purge_subject, volume.volume_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load recovered durable binding");
        assert_eq!(recovered_binding.0, replacement_process_instance_id);
        assert_eq!(
            recovered_binding.1,
            runner_purge_subject_sha256(&grant.purge_subject).expect("hash recovered subject")
        );
        assert_eq!(recovered_binding.2, 1);

        success
            .pool
            .get()
            .expect("open stale-process mutation connection")
            .execute(
                "UPDATE jobs_runner_volumes SET active_instance_id = ?2 \
                  WHERE volume_id = ?1",
                params![volume.volume_id, encode_base64url(&[202; 32])],
            )
            .expect("supersede the process bound to the execution lease");
        assert!(matches!(
            finish_execution_lease(
                &success.pool,
                "acct-claim-success",
                &application_id,
                &run_id,
                &recovered_grant.lease.lease_token,
                recovered_grant.lease.fence,
                "released",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert!(matches!(
            reconcile_execution_lease_checkpoint(
                &success.pool,
                "acct-claim-success",
                &application_id,
                &run_id,
                "worker-claim-success",
                Some(&recovered_grant.lease.lease_token),
                recovered_grant.lease.fence,
                2,
                "prepared",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));

        let failure = TestDatabase::new(&["acct-claim-failure"]);
        let (application_id, run_id, browser_profile_id) = runner_execution_lease_fixture(
            &failure.pool,
            "acct-claim-failure",
            "runner-purge-0@example.test",
            "runner-volume-failure",
        );
        let volume = fixed_volume(&failure.pool, 28, "claim-failure", 10);
        let failure_now = now_ms();
        claim_runner_volume_instance(
            &failure.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            failure_now,
            failure_now.saturating_add(300_000),
        )
        .expect("claim failure-case runner process");
        attest_fixed_volume(&failure.pool, &volume, failure_now.saturating_add(1));
        activate_reconciled_runner_volume(
            &failure.pool,
            &volume.volume_id,
            1,
            &volume.process_instance_id,
            failure_now.saturating_add(1),
        )
        .expect("activate failure-case volume locally");
        let failed_request = BindRunnerVolumeResidencyRequest {
            account_id: "acct-claim-failure".to_string(),
            run_id: run_id.clone(),
            worker_id: "worker-claim-failure".to_string(),
            volume_id: volume.volume_id,
            enrollment_epoch: 1,
            process_instance_id: volume.process_instance_id,
            now_ms: failure_now.saturating_add(2),
        };
        assert!(claim_execution_lease_for_runner_volume(
            &failure.pool,
            "acct-claim-failure",
            &application_id,
            &run_id,
            &browser_profile_id,
            "worker-claim-failure",
            &failed_request,
        )
        .is_err());
        let rollback_counts: (i64, i64, i64, i64) = failure
            .pool
            .get()
            .expect("open rollback verification connection")
            .query_row(
                "SELECT \
                    (SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1), \
                    (SELECT COUNT(*) FROM jobs_execution_lease_volume_bindings \
                      WHERE run_id = ?1), \
                    (SELECT COUNT(*) FROM jobs_runner_account_subjects \
                      WHERE account_id = 'acct-claim-failure'), \
                    (SELECT COUNT(*) FROM jobs_runner_volume_residencies)",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("load rolled-back claim rows");
        assert_eq!(rollback_counts, (0, 0, 0, 0));
    }

    #[test]
    fn destruction_requires_fresh_quiescent_volume_state_and_replays_exactly() {
        let database = TestDatabase::new(&[]);
        let volume = fixed_volume(&database.pool, 29, "destruction-freshness", 10);
        let mut destruction = RecordRunnerVolumeDestructionRequest {
            destruction_id: "destruction-freshness".to_string(),
            expected_enrollment_generation: 1,
            volume_id: volume.volume_id.clone(),
            volume_epoch: 1,
            volume_key_fingerprint: volume.key_fingerprint.clone(),
            provider: "test-provider".to_string(),
            provider_resource_id: volume.provider_resource_id.clone(),
            resource_fingerprint: volume.resource_fingerprint.clone(),
            evidence_type: "provider_volume_destroyed".to_string(),
            snapshot_inventory_sha256: EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
            evidence_sha256: sha256(b"destruction-freshness"),
            authorization_ref: "destroy-authorization-freshness".to_string(),
            authorized_by: "test-operator".to_string(),
            occurred_at_ms: 9,
            recorded_at_ms: 11,
            details: json!({"providerReceipt": "receipt-freshness"}),
        };
        assert!(matches!(
            record_runner_volume_destruction(&database.pool, &destruction),
            Err(RunnerVolumePurgeError::Conflict)
        ));

        claim_fixed_volume(&database.pool, &volume, 20);
        destruction.occurred_at_ms = 20;
        destruction.recorded_at_ms = 21;
        assert!(matches!(
            record_runner_volume_destruction(&database.pool, &destruction),
            Err(RunnerVolumePurgeError::Conflict)
        ));

        destruction.occurred_at_ms = 900_000;
        destruction.recorded_at_ms = 900_001;
        let applied = record_runner_volume_destruction(&database.pool, &destruction)
            .expect("record fresh evidence after the process lease expires");
        assert_eq!(applied.disposition, RunnerVolumeWriteDisposition::Applied);
        let mut replay = destruction.clone();
        replay.destruction_id = "destruction-freshness-retry".to_string();
        replay.recorded_at_ms = 900_002;
        let replayed = record_runner_volume_destruction(&database.pool, &replay)
            .expect("replay the exact immutable destruction evidence");
        assert_eq!(replayed.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replayed.destruction_id, destruction.destruction_id);
        assert_eq!(replayed.recorded_at_ms, destruction.recorded_at_ms);
    }

    #[test]
    fn fanout_is_sorted_conservative_and_immutable_after_new_enrollment() {
        let database = TestDatabase::new(&["acct-freeze", "acct-later"]);
        let signer = server_signer();
        let volume_a = fixed_volume(&database.pool, 21, "freeze-a", 10);
        let volume_b = fixed_volume(&database.pool, 22, "freeze-b", 11);
        let destroyed = fixed_volume(&database.pool, 23, "freeze-destroyed", 12);
        claim_fixed_volume(&database.pool, &volume_a, 13);
        let offline = lookup_runner_volume(&database.pool, &volume_b.volume_id)
            .expect("load offline frozen volume")
            .expect("offline frozen volume exists");
        assert!(offline.active_instance_id.is_none());
        let offline_attestation_count: i64 = database
            .pool
            .get()
            .expect("open offline attestation count connection")
            .query_row(
                "SELECT COUNT(*) FROM jobs_runner_volume_storage_attestations \
                  WHERE volume_id = ?1",
                params![volume_b.volume_id],
                |row| row.get(0),
            )
            .expect("count offline storage attestations");
        assert_eq!(offline_attestation_count, 0);
        let destruction = RecordRunnerVolumeDestructionRequest {
            destruction_id: "destruction-freeze".to_string(),
            expected_enrollment_generation: 3,
            volume_id: destroyed.volume_id.clone(),
            volume_epoch: 1,
            volume_key_fingerprint: destroyed.key_fingerprint.clone(),
            provider: "test-provider".to_string(),
            provider_resource_id: destroyed.provider_resource_id.clone(),
            resource_fingerprint: destroyed.resource_fingerprint.clone(),
            evidence_type: "provider_volume_destroyed".to_string(),
            snapshot_inventory_sha256: EMPTY_RUNNER_INVENTORY_SHA256.to_string(),
            evidence_sha256: sha256(b"destruction-freeze"),
            authorization_ref: "destroy-authorization".to_string(),
            authorized_by: "test-operator".to_string(),
            occurred_at_ms: 20,
            recorded_at_ms: 21,
            details: json!({"providerReceipt": "receipt-freeze"}),
        };
        let recorded = record_runner_volume_destruction(&database.pool, &destruction)
            .expect("record typed destruction evidence");
        assert_eq!(recorded.disposition, RunnerVolumeWriteDisposition::Applied);
        let mut destruction_retry = destruction.clone();
        destruction_retry.destruction_id = "new-server-retry-id".to_string();
        destruction_retry.recorded_at_ms = 22;
        let destruction_replay =
            record_runner_volume_destruction(&database.pool, &destruction_retry)
                .expect("replay destruction evidence with new server fields");
        assert_eq!(
            destruction_replay.disposition,
            RunnerVolumeWriteDisposition::Replay
        );
        assert_eq!(
            destruction_replay.destruction_id,
            destruction.destruction_id
        );
        assert_eq!(
            destruction_replay.recorded_at_ms,
            destruction.recorded_at_ms
        );
        let prepared = prepare(&database, &signer, "acct-freeze", "request-freeze", 30, 41);
        assert_eq!(prepared.status.required_target_count, 2);
        let ids = prepared
            .commands
            .iter()
            .map(|command| command.target_volume_id.as_str())
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        assert!(ids.contains(&volume_a.volume_id.as_str()));
        assert!(ids.contains(&volume_b.volume_id.as_str()));
        assert!(!ids.contains(&destroyed.volume_id.as_str()));
        let replay = prepare_runner_volume_purge_with_subject_material(
            &database.pool,
            &signer,
            &server_key_ring(&signer),
            &PrepareRunnerVolumePurgeRequest {
                request_id: "request-freeze".to_string(),
                account_id: "acct-freeze".to_string(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                expected_legacy_inventory_generation: prepared.status.legacy_inventory_generation,
                expected_legacy_inventory_reconciliation_id: prepared
                    .status
                    .legacy_inventory_reconciliation_id
                    .clone(),
                expected_legacy_inventory_authority_id: prepared
                    .status
                    .legacy_inventory_authority_id
                    .clone(),
                expected_legacy_inventory_authority_sha256: prepared
                    .status
                    .legacy_inventory_authority_sha256
                    .clone(),
                now_ms: 30,
            },
            [99_u8; 32],
        )
        .expect("replay frozen fanout");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);
        assert_eq!(replay.commands, prepared.commands);
        let later = fixed_volume(&database.pool, 24, "freeze-later", 40);
        claim_fixed_volume(&database.pool, &later, 41);
        let unchanged = runner_volume_purge_status(&database.pool, "request-freeze")
            .expect("load frozen request");
        assert_eq!(unchanged.required_target_count, 2);
        let later_request = prepare(&database, &signer, "acct-later", "request-later", 50, 42);
        assert_eq!(later_request.status.required_target_count, 3);
        assert!(later_request
            .commands
            .iter()
            .any(|command| command.target_volume_id == later.volume_id));
        let fleet = runner_volume_fleet_status(&database.pool).expect("load fleet status");
        assert_eq!(fleet.purge_generation, 2);
        assert_eq!(fleet.tombstone_generation, 0);
    }

    #[test]
    fn legacy_browser_session_blocks_zero_target_completion() {
        let database = TestDatabase::new(&["acct-legacy"]);
        database
            .pool
            .get()
            .expect("open test connection")
            .execute(
                "INSERT INTO jobs_browser_sessions ( \
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms \
                 ) VALUES ('legacy-session', 'acct-legacy', 'cloud', 'complete', '{}', 1, 1)",
                [],
            )
            .expect("seed pre-602 browser session");
        let prepared = prepare(
            &database,
            &server_signer(),
            "acct-legacy",
            "request-legacy",
            10,
            51,
        );
        assert_eq!(prepared.status.required_target_count, 0);
        assert!(prepared.status.legacy_unresolved_count > 0);
        assert!(matches!(
            complete_runner_volume_purge(&database.pool, "request-legacy", 11),
            Err(RunnerVolumePurgeError::NotReady)
        ));
        let resolution = ResolveRunnerPurgeLegacyRequest {
            request_id: "request-legacy".to_string(),
            expected_legacy_unresolved_count: prepared.status.legacy_unresolved_count,
            resolution_ref: "legacy-audit-ref".to_string(),
            resolution_sha256: sha256(b"legacy-audit-evidence"),
            resolved_by: "test-operator".to_string(),
            resolved_at_ms: 12,
        };
        let mut stale = resolution.clone();
        stale.expected_legacy_unresolved_count = stale
            .expected_legacy_unresolved_count
            .checked_add(1)
            .expect("increment legacy count");
        assert!(matches!(
            resolve_runner_volume_purge_legacy(&database.pool, &stale),
            Err(RunnerVolumePurgeError::Conflict)
        ));
        assert_eq!(
            resolve_runner_volume_purge_legacy(&database.pool, &resolution)
                .expect("resolve exact legacy snapshot")
                .0,
            RunnerVolumeWriteDisposition::Applied
        );
        let mut retry = resolution;
        retry.resolved_at_ms = 13;
        assert_eq!(
            resolve_runner_volume_purge_legacy(&database.pool, &retry)
                .expect("replay legacy resolution with new server time")
                .0,
            RunnerVolumeWriteDisposition::Replay
        );
        complete_runner_volume_purge(&database.pool, "request-legacy", 14)
            .expect("complete resolved zero-target purge");
    }

    #[test]
    fn acknowledgement_replay_is_exact_and_conflicts_fail_closed() {
        let database = TestDatabase::new(&["acct-ack"]);
        let signer = server_signer();
        let volume = fixed_volume(&database.pool, 31, "ack", 10);
        claim_fixed_volume(&database.pool, &volume, 11);
        let prepared = prepare(&database, &signer, "acct-ack", "request-ack", 20, 61);
        let ack = signed_ack(&volume, command_for(&prepared, &volume), 21);
        let mut mismatched_generic_fields = ack.clone();
        mismatched_generic_fields.before_inventory_count += 1;
        assert!(matches!(
            verify_runner_purge_ack(
                &mismatched_generic_fields,
                &encode_base64url(volume.signing_key.verifying_key().as_bytes()),
            ),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));
        let mut mismatched_evidence_digest = ack.clone();
        mismatched_evidence_digest
            .storage_evidence
            .root
            .before
            .sha256 = sha256("root drift");
        assert!(matches!(
            verify_runner_purge_ack(
                &mismatched_evidence_digest,
                &encode_base64url(volume.signing_key.verifying_key().as_bytes()),
            ),
            Err(RunnerVolumePurgeError::InvalidRequest)
        ));
        let key_ring = server_key_ring(&signer);
        assert!(
            !runner_build_satisfies(&ack.runner_build_id, "runner-603"),
            "a later global policy floor must not replace the frozen command floor"
        );
        let applied = acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 21)
            .expect("acknowledge purge");
        assert_eq!(applied.disposition, RunnerVolumeWriteDisposition::Applied);
        let replay = acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 22)
            .expect("replay exact acknowledgement");
        assert_eq!(replay.disposition, RunnerVolumeWriteDisposition::Replay);

        advance_legacy_inventory_ready(&database.pool, "ack-successor", 23);
        let successor_fleet =
            runner_volume_fleet_status(&database.pool).expect("load ACK successor authority");
        let successor = prepare_runner_volume_purge(
            &database.pool,
            &signer,
            &key_ring,
            &PrepareRunnerVolumePurgeRequest {
                request_id: "request-ack".to_string(),
                account_id: "acct-ack".to_string(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                expected_legacy_inventory_generation: successor_fleet.legacy_inventory_generation,
                expected_legacy_inventory_reconciliation_id: successor_fleet
                    .legacy_inventory_reconciliation_id
                    .expect("ACK successor reconciliation id"),
                expected_legacy_inventory_authority_id: successor_fleet
                    .legacy_inventory_authority_id
                    .expect("ACK successor authority id"),
                expected_legacy_inventory_authority_sha256: successor_fleet
                    .legacy_inventory_authority_sha256
                    .expect("ACK successor authority digest"),
                now_ms: 25,
            },
        )
        .expect("prepare ACK successor request");
        assert_ne!(successor.status.request_id, ack.request_id);
        assert_eq!(
            runner_volume_purge_status(&database.pool, &ack.request_id)
                .expect("load superseded ACK request")
                .state,
            "superseded"
        );
        let replay_after_supersession =
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 26)
                .expect("replay exact acknowledgement after request supersession");
        assert_eq!(
            replay_after_supersession.disposition,
            RunnerVolumeWriteDisposition::Replay
        );
        let mut changed_input = NewRunnerPurgeAck {
            request_id: ack.request_id.clone(),
            command_id: ack.command_id.clone(),
            command_sha256: ack.command_sha256.clone(),
            target_volume_id: ack.target_volume_id.clone(),
            target_key_fingerprint: ack.target_key_fingerprint.clone(),
            enrollment_epoch: ack.enrollment_epoch,
            process_instance_id: ack.process_instance_id.clone(),
            purge_subject_sha256: ack.purge_subject_sha256.clone(),
            purge_generation: ack.purge_generation,
            storage_evidence: test_storage_evidence("changed-inventory", 2),
            storage_evidence_sha256: String::new(),
            before_inventory_count: 0,
            before_inventory_sha256: String::new(),
            after_inventory_count: 0,
            after_inventory_sha256: String::new(),
            removed_count: 0,
            runner_build_id: ack.runner_build_id.clone(),
            completed_at_ms: ack.completed_at_ms,
        };
        changed_input.storage_evidence_sha256 = changed_input
            .storage_evidence
            .sha256()
            .expect("hash changed storage evidence");
        let changed_before = changed_input
            .storage_evidence
            .target_inventory_state(true)
            .expect("derive changed before inventory");
        let changed_after = changed_input
            .storage_evidence
            .target_inventory_state(false)
            .expect("derive changed after inventory");
        changed_input.before_inventory_count = changed_before.0;
        changed_input.before_inventory_sha256 = changed_before.1;
        changed_input.after_inventory_count = changed_after.0;
        changed_input.after_inventory_sha256 = changed_after.1;
        changed_input.removed_count = changed_before.0 - changed_after.0;
        let changed = sign_runner_purge_ack(&volume.signing_key, changed_input.clone())
            .expect("sign conflicting acknowledgement");
        assert!(matches!(
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &changed, 27),
            Err(RunnerVolumePurgeError::Conflict)
        ));
        changed_input.storage_evidence.root.before.sha256 = sha256(b"forged inventory");
        changed_input.storage_evidence_sha256 = changed_input
            .storage_evidence
            .sha256()
            .expect("hash forged storage evidence");
        let forged_key = Ed25519SigningKey::from_bytes(&[222_u8; 32]);
        let forged =
            sign_runner_purge_ack(&forged_key, changed_input).expect("sign forged acknowledgement");
        assert!(matches!(
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &forged, 27),
            Err(RunnerVolumePurgeError::Unauthorized)
        ));

        let replacement_process = encode_base64url(&[199_u8; 32]);
        claim_runner_volume_instance(
            &database.pool,
            &volume.volume_id,
            1,
            &replacement_process,
            900_001,
            1_000_000,
        )
        .expect("claim replacement process after the original lease expires");
        let replay_after_restart =
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 900_002)
                .expect("exact committed ACK replays after process replacement");
        assert_eq!(
            replay_after_restart.disposition,
            RunnerVolumeWriteDisposition::Replay
        );
    }

    #[test]
    fn out_of_order_completion_generations_do_not_skip_late_tombstone() {
        let database = TestDatabase::new(&["acct-order-a", "acct-order-b"]);
        let signer = server_signer();
        let key_ring = server_key_ring(&signer);
        let volume_one = fixed_volume(&database.pool, 41, "order-one", 10);
        claim_fixed_volume(&database.pool, &volume_one, 11);
        let purge_a = prepare(
            &database,
            &signer,
            "acct-order-a",
            "request-order-a",
            20,
            71,
        );
        assert_eq!(purge_a.status.purge_generation, 1);
        let volume_two = fixed_volume(&database.pool, 42, "order-two", 30);
        claim_fixed_volume(&database.pool, &volume_two, 31);
        let purge_b = prepare(
            &database,
            &signer,
            "acct-order-b",
            "request-order-b",
            40,
            72,
        );
        assert_eq!(purge_b.status.purge_generation, 2);
        for volume in [&volume_one, &volume_two] {
            let ack = signed_ack(volume, command_for(&purge_b, volume), 50);
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 50)
                .expect("acknowledge purge B target");
        }
        let complete_b = complete_runner_volume_purge(&database.pool, "request-order-b", 60)
            .expect("complete purge B first");
        assert_eq!(complete_b.tombstone_generation, 1);
        let after_b = lookup_runner_volume(&database.pool, &volume_two.volume_id)
            .expect("lookup volume two")
            .expect("volume two exists");
        assert_eq!(after_b.required_tombstone_generation, 1);
        assert_eq!(after_b.reconciled_tombstone_generation, 1);
        attest_fixed_volume(&database.pool, &volume_one, 61);
        attest_fixed_volume(&database.pool, &volume_two, 62);
        let successor_fleet =
            runner_volume_fleet_status(&database.pool).expect("load request-A successor fleet");
        let purge_a_successor = prepare_runner_volume_purge(
            &database.pool,
            &signer,
            &key_ring,
            &PrepareRunnerVolumePurgeRequest {
                request_id: "request-order-a".to_string(),
                account_id: "acct-order-a".to_string(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                expected_legacy_inventory_generation: successor_fleet.legacy_inventory_generation,
                expected_legacy_inventory_reconciliation_id: successor_fleet
                    .legacy_inventory_reconciliation_id
                    .expect("request-A successor reconciliation id"),
                expected_legacy_inventory_authority_id: successor_fleet
                    .legacy_inventory_authority_id
                    .expect("request-A successor authority id"),
                expected_legacy_inventory_authority_sha256: successor_fleet
                    .legacy_inventory_authority_sha256
                    .expect("request-A successor authority digest"),
                now_ms: 65,
            },
        )
        .expect("prepare request-A successor after request-B completion");
        assert_ne!(
            purge_a_successor.status.request_id,
            purge_a.status.request_id
        );
        assert_eq!(
            runner_volume_purge_status(&database.pool, &purge_a.status.request_id)
                .expect("load stale-authority request A")
                .state,
            "superseded"
        );
        for volume in [&volume_one, &volume_two] {
            let ack = signed_ack(volume, command_for(&purge_a_successor, volume), 70);
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 70)
                .expect("acknowledge reauthorized purge A target");
        }
        let complete_a =
            complete_runner_volume_purge(&database.pool, &purge_a_successor.status.request_id, 80)
                .expect("complete reauthorized purge A later");
        assert_eq!(complete_a.tombstone_generation, 2);

        // A volume enrolled after both completions must receive retained
        // tombstones in completion order (B then A), not request order (A
        // then B). This is the online high-watermark safety invariant.
        let volume_three = fixed_volume(&database.pool, 43, "order-three", 81);
        claim_fixed_volume(&database.pool, &volume_three, 82);
        let retained_page_one = poll_runner_volume_purge_commands(
            &database.pool,
            &signer,
            &key_ring,
            &PollRunnerVolumePurgeCommandsRequest {
                volume_id: volume_three.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume_three.process_instance_id.clone(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                now_ms: 83,
                limit: 1,
                after_command_id: None,
            },
        )
        .expect("poll first retained tombstone page");
        assert_eq!(retained_page_one.commands.len(), 1);
        let cursor = retained_page_one
            .next_after_command_id
            .clone()
            .expect("first retained page cursor");
        assert_eq!(cursor, retained_page_one.commands[0].command_id);
        let retained_page_two = poll_runner_volume_purge_commands(
            &database.pool,
            &signer,
            &key_ring,
            &PollRunnerVolumePurgeCommandsRequest {
                volume_id: volume_three.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume_three.process_instance_id.clone(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                now_ms: 84,
                limit: 1,
                after_command_id: Some(cursor),
            },
        )
        .expect("poll second retained tombstone page");
        assert_eq!(retained_page_two.commands.len(), 1);
        assert_eq!(retained_page_two.next_after_command_id, None);
        for invalid_cursor in [
            "missing-command-cursor".to_string(),
            command_for(&purge_a_successor, &volume_one)
                .command_id
                .clone(),
        ] {
            assert!(matches!(
                poll_runner_volume_purge_commands(
                    &database.pool,
                    &signer,
                    &key_ring,
                    &PollRunnerVolumePurgeCommandsRequest {
                        volume_id: volume_three.volume_id.clone(),
                        enrollment_epoch: 1,
                        process_instance_id: volume_three.process_instance_id.clone(),
                        minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                        now_ms: 84,
                        limit: 1,
                        after_command_id: Some(invalid_cursor),
                    },
                ),
                Err(RunnerVolumePurgeError::InvalidRequest)
            ));
        }
        let retained = [
            retained_page_one.commands[0].clone(),
            retained_page_two.commands[0].clone(),
        ];
        assert_eq!(retained[0].request_id, purge_b.status.request_id);
        assert_eq!(retained[1].request_id, purge_a_successor.status.request_id);
        assert_eq!(
            (
                retained[0].legacy_inventory_authority_generation,
                retained[0].legacy_inventory_authority_sha256.as_str(),
            ),
            (
                purge_b.status.legacy_inventory_generation,
                purge_b.status.legacy_inventory_authority_sha256.as_str(),
            )
        );
        assert_eq!(
            (
                retained[1].legacy_inventory_authority_generation,
                retained[1].legacy_inventory_authority_sha256.as_str(),
            ),
            (
                purge_a_successor.status.legacy_inventory_generation,
                purge_a_successor
                    .status
                    .legacy_inventory_authority_sha256
                    .as_str(),
            )
        );
        for (index, command) in retained.iter().enumerate() {
            let ack = signed_ack(&volume_three, command, 85 + index as i64);
            acknowledge_runner_volume_purge(&database.pool, &key_ring, &ack, 85 + index as i64)
                .expect("acknowledge retained tombstone in completion order");
        }
        let retained_reconciled = lookup_runner_volume(&database.pool, &volume_three.volume_id)
            .expect("lookup retained-tombstone volume")
            .expect("retained-tombstone volume exists");
        assert_eq!(retained_reconciled.required_tombstone_generation, 2);
        assert_eq!(retained_reconciled.reconciled_tombstone_generation, 2);

        let frozen_target_reconciled = lookup_runner_volume(&database.pool, &volume_two.volume_id)
            .expect("lookup volume two")
            .expect("volume two exists");
        assert_eq!(frozen_target_reconciled.required_tombstone_generation, 2);
        assert_eq!(frozen_target_reconciled.reconciled_tombstone_generation, 2);
        assert_eq!(frozen_target_reconciled.status, "reconciling");
        let commands = poll_runner_volume_purge_commands(
            &database.pool,
            &signer,
            &key_ring,
            &PollRunnerVolumePurgeCommandsRequest {
                volume_id: volume_two.volume_id.clone(),
                enrollment_epoch: 1,
                process_instance_id: volume_two.process_instance_id.clone(),
                minimum_runner_build_id: TEST_RUNNER_BUILD.to_string(),
                now_ms: 90,
                limit: 10,
                after_command_id: None,
            },
        )
        .expect("poll fully reconciled frozen target");
        assert!(commands.commands.is_empty());
    }
}
