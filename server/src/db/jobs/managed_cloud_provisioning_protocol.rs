//! Pure Phase648 wire foundation, not provisioning or business-effect authority.
//!
//! Parsed references still require signed process context and current database joins.
//! These codecs do not verify probes, resource ownership, queue isolation or readiness.
//! Runtime integration and the remaining evidence contracts require separate review.

use super::{
    managed_cloud_base64url_32, managed_cloud_canonical_json, managed_cloud_decode_exact,
    managed_cloud_hex64, managed_cloud_parse_canonical, managed_cloud_route_id,
    managed_cloud_runtime_role, managed_cloud_safe_integer, managed_cloud_sha256,
    managed_cloud_token, require_runner_identifier, ManagedCloudRegistryError, ManagedCloudResult,
};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::Sha256;
use std::net::Ipv4Addr;
use unicode_normalization::UnicodeNormalization;

const MAX_PROBE_BYTES: usize = 64 * 1024;
const SESSION_DOMAIN: &[u8] = b"bluey-jobs-managed-cloud-runtime-session-v2\0";
const DEPENDENCY_DOMAIN: &[u8] = b"bluey-jobs-managed-cloud-dependency-evidence-v2\0";
const OBSERVATION_DOMAIN: &[u8] = b"bluey-jobs-managed-cloud-runtime-observation-v2\0";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum AuthorityKind {
    #[serde(rename = "provisioning_v4")]
    ProvisioningV4,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Purpose {
    Production,
    NonproductionSynthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthorityBinding {
    pub(crate) authority_kind: AuthorityKind,
    pub(crate) purpose: Purpose,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) context_sha256: Option<String>,
    pub(crate) queue_generation_id: String,
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl AuthorityBinding {
    fn validate(&self) -> ManagedCloudResult<()> {
        let valid_context = match (self.purpose, self.context_sha256.as_deref()) {
            (Purpose::Production, None) => true,
            (Purpose::NonproductionSynthetic, Some(hash)) => managed_cloud_hex64(hash),
            _ => false,
        };
        require(valid_context && managed_cloud_route_id(&self.queue_generation_id))
    }
}

pub(crate) fn canonical_binding_bytes(binding: &AuthorityBinding) -> ManagedCloudResult<Vec<u8>> {
    binding.validate()?;
    managed_cloud_canonical_json(binding)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ListenerState {
    Listening,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServerState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerState {
    Running,
    NotRunning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LoopState {
    Parked,
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ControlProgress {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleState {
    Parked,
    LatchAcknowledged,
    Operational,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum TlsTransport {
    #[serde(rename = "tls")]
    Tls,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum HttpsTransport {
    #[serde(rename = "https")]
    Https,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NamespaceState {
    Registered,
    NotRegistered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TemporalNamespace {
    pub(crate) endpoint_id: String,
    pub(crate) transport: TlsTransport,
    pub(crate) authority: String,
    pub(crate) namespace: String,
    pub(crate) namespace_id: String,
    pub(crate) state: NamespaceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GatewayControl {
    pub(crate) endpoint_id: String,
    pub(crate) transport: HttpsTransport,
    pub(crate) authority: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) instance_epoch: i64,
    pub(crate) head_revision: i64,
    pub(crate) transition_sha256: String,
    pub(crate) lifecycle_state: LifecycleState,
    pub(crate) observation_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BootStartup {
    pub(crate) boot_id: String,
    pub(crate) boot_generation: i64,
    pub(crate) startup_request_id: String,
    pub(crate) parked_heartbeat_sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VolumeBinding {
    pub(crate) volume_id: String,
    pub(crate) enrollment_epoch: i64,
    pub(crate) process_instance_id: String,
    pub(crate) runtime_grant_id: String,
    pub(crate) runtime_sha256: String,
    pub(crate) storage_attestation_generation: i64,
    pub(crate) storage_attestation_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ApiProbe {
    pub(crate) version: u8,
    pub(crate) listener_state: ListenerState,
    pub(crate) server_state: ServerState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DispatcherProbe {
    pub(crate) version: u8,
    pub(crate) loop_state: LoopState,
    pub(crate) control_progress: ControlProgress,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) gateway_control: Option<GatewayControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GatewayProbe {
    pub(crate) version: u8,
    pub(crate) listener_state: ListenerState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) temporal_namespace: Option<TemporalNamespace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkerProbe {
    pub(crate) version: u8,
    pub(crate) worker_state: WorkerState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) temporal_namespace: Option<TemporalNamespace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ControlProbe {
    pub(crate) version: u8,
    pub(crate) loop_state: LoopState,
    pub(crate) control_progress: ControlProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceVerifierProbe {
    pub(crate) version: u8,
    pub(crate) loop_state: LoopState,
    pub(crate) control_progress: ControlProgress,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) boot_startup: Option<BootStartup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunnerProbe {
    pub(crate) version: u8,
    pub(crate) listener_state: ListenerState,
    pub(crate) storage_recovery_complete: bool,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) volume_binding: Option<VolumeBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DependencyProbe {
    JobsApi(ApiProbe),
    WorkflowCommandDispatcher(DispatcherProbe),
    WorkflowCleanupDispatcher(DispatcherProbe),
    WorkflowGateway(GatewayProbe),
    WorkflowWorker(WorkerProbe),
    DiscoveryWorker(ControlProbe),
    GlobalDiscoveryWorker(ControlProbe),
    OriginalSourceVerifier(SourceVerifierProbe),
    ManagedRunner(RunnerProbe),
}

impl DependencyProbe {
    pub(crate) fn role(&self) -> &'static str {
        match self {
            Self::JobsApi(_) => "jobs_api",
            Self::WorkflowCommandDispatcher(_) => "workflow_command_dispatcher",
            Self::WorkflowCleanupDispatcher(_) => "workflow_cleanup_dispatcher",
            Self::WorkflowGateway(_) => "workflow_gateway",
            Self::WorkflowWorker(_) => "workflow_worker",
            Self::DiscoveryWorker(_) => "discovery_worker",
            Self::GlobalDiscoveryWorker(_) => "global_discovery_worker",
            Self::OriginalSourceVerifier(_) => "original_source_verifier",
            Self::ManagedRunner(_) => "managed_runner",
        }
    }

    fn validate(&self) -> ManagedCloudResult<()> {
        match self {
            Self::JobsApi(probe) => require(probe.version == 1),
            Self::WorkflowCommandDispatcher(probe) | Self::WorkflowCleanupDispatcher(probe) => {
                validate_control(probe.version, probe.loop_state, probe.control_progress)?;
                if let Some(gateway) = &probe.gateway_control {
                    require(
                        managed_cloud_route_id(&gateway.endpoint_id)
                            && canonical_authority(&gateway.authority)
                            && managed_cloud_route_id(&gateway.runtime_instance_id)
                            && managed_cloud_safe_integer(gateway.instance_epoch, true)
                            && managed_cloud_safe_integer(gateway.head_revision, true)
                            && managed_cloud_hex64(&gateway.transition_sha256)
                            && managed_cloud_hex64(&gateway.observation_sha256),
                    )?;
                }
                Ok(())
            }
            Self::WorkflowGateway(probe) => {
                require(probe.version == 1)?;
                validate_namespace(probe.temporal_namespace.as_ref())
            }
            Self::WorkflowWorker(probe) => {
                require(probe.version == 1)?;
                validate_namespace(probe.temporal_namespace.as_ref())
            }
            Self::DiscoveryWorker(probe) | Self::GlobalDiscoveryWorker(probe) => {
                validate_control(probe.version, probe.loop_state, probe.control_progress)
            }
            Self::OriginalSourceVerifier(probe) => {
                validate_control(probe.version, probe.loop_state, probe.control_progress)?;
                if let Some(boot) = &probe.boot_startup {
                    require(
                        osv_identifier(&boot.boot_id)
                            && osv_identifier(&boot.startup_request_id)
                            && managed_cloud_safe_integer(boot.boot_generation, true)
                            && managed_cloud_safe_integer(boot.parked_heartbeat_sequence, true),
                    )?;
                }
                Ok(())
            }
            Self::ManagedRunner(probe) => {
                require(probe.version == 1)?;
                if let Some(volume) = &probe.volume_binding {
                    require(
                        managed_cloud_base64url_32(&volume.volume_id)
                            && managed_cloud_base64url_32(&volume.process_instance_id)
                            && require_runner_identifier(&volume.runtime_grant_id).is_ok()
                            && managed_cloud_safe_integer(volume.enrollment_epoch, true)
                            && managed_cloud_safe_integer(
                                volume.storage_attestation_generation,
                                true,
                            )
                            && managed_cloud_hex64(&volume.runtime_sha256)
                            && managed_cloud_hex64(&volume.storage_attestation_sha256),
                    )?;
                }
                Ok(())
            }
        }
    }
}

fn validate_control(
    version: u8,
    state: LoopState,
    progress: ControlProgress,
) -> ManagedCloudResult<()> {
    require(version == 1 && (state != LoopState::Stopped || progress == ControlProgress::Stale))
}

fn validate_namespace(namespace: Option<&TemporalNamespace>) -> ManagedCloudResult<()> {
    if let Some(namespace) = namespace {
        require(
            managed_cloud_route_id(&namespace.endpoint_id)
                && canonical_authority(&namespace.authority)
                && bounded_text(&namespace.namespace, 240)
                && bounded_text(&namespace.namespace_id, 128),
        )?;
    }
    Ok(())
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.starts_with('\u{feff}')
        && !value.ends_with('\u{feff}')
        && !value.contains('\u{fffd}')
        && !value.chars().any(char::is_control)
        && value.nfc().eq(value.chars())
}

fn osv_identifier(value: &str) -> bool {
    value.len() >= 20 && managed_cloud_token(value, 128)
}

// Initial syntax slice supports canonical DNS/IPv4 only, not IPv6 or SSRF admission.
// Actual endpoint/context authorization and TLS verification are not performed here.
fn canonical_authority(value: &str) -> bool {
    if value.len() > 253 || !value.is_ascii() {
        return false;
    }
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    if host.is_empty()
        || host.contains(':')
        || port.is_empty()
        || port.starts_with('0')
        || !port.bytes().all(|byte| byte.is_ascii_digit())
        || !port.parse::<u16>().is_ok_and(|port| port > 0)
    {
        return false;
    }
    if let Ok(address) = host.parse::<Ipv4Addr>() {
        return address.to_string() == host;
    }
    let last = host.rsplit('.').next().unwrap_or_default();
    if last.bytes().all(|byte| byte.is_ascii_digit())
        || last
            .strip_prefix("0x")
            .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return false;
    }
    host.split('.').all(|label| {
        (1..=63).contains(&label.len())
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    })
}

fn require(valid: bool) -> ManagedCloudResult<()> {
    if valid {
        Ok(())
    } else {
        Err(ManagedCloudRegistryError::InvalidAuthority)
    }
}

pub(crate) fn parse_dependency_probe(bytes: &[u8]) -> ManagedCloudResult<DependencyProbe> {
    require(!bytes.is_empty() && bytes.len() <= MAX_PROBE_BYTES)?;
    let probe: DependencyProbe = managed_cloud_parse_canonical(bytes)?;
    probe.validate()?;
    Ok(probe)
}

pub(crate) fn canonical_probe_bytes(probe: &DependencyProbe) -> ManagedCloudResult<Vec<u8>> {
    probe.validate()?;
    let bytes = managed_cloud_canonical_json(probe)?;
    require(bytes.len() <= MAX_PROBE_BYTES)?;
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HeartbeatHealthState {
    Ready,
    Degraded,
    Draining,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HeartbeatReasonCode {
    ArtifactMismatch,
    ConfigMismatch,
    DependencyUnavailable,
    Draining,
    HeadMismatch,
    MigrationMismatch,
    ProbeFailed,
    ProtocolMismatch,
    Startup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueueObservation {
    pub(crate) poller_identity_sha256: String,
    pub(crate) backlog_count: i64,
    pub(crate) delivered_task_count: i64,
}

fn optional_nonnull<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Closed v2 HTTP shape only, not lifecycle eligibility or accepted storage.
/// Nullable fields must be present; the OSV boot pair must be absent or non-null.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HeartbeatRequest {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) runtime_instance_id: String,
    pub(crate) worker_id: String,
    pub(crate) session_token: String,
    pub(crate) heartbeat_sequence: i64,
    pub(crate) observed_head_revision: i64,
    pub(crate) observed_transition_sha256: String,
    pub(crate) activation_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) component_id: String,
    pub(crate) role: String,
    pub(crate) artifact_sha256: String,
    pub(crate) migration_set_sha256: String,
    pub(crate) config_schema_sha256: String,
    pub(crate) protocol_set_sha256: String,
    pub(crate) task_queue_sha256: String,
    pub(crate) failure_converter_sha256: String,
    pub(crate) dependency_evidence_sha256: String,
    pub(crate) lifecycle_state: LifecycleState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) acknowledged_latch_sha256: Option<String>,
    pub(crate) dependency_probe: DependencyProbe,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) queue_observation: Option<QueueObservation>,
    pub(crate) health_state: HeartbeatHealthState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) reason_code: Option<HeartbeatReasonCode>,
    #[serde(
        default,
        deserialize_with = "optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) boot_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "optional_nonnull",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) boot_generation: Option<i64>,
}

impl std::fmt::Debug for HeartbeatRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never expose the session or any unvalidated request value in diagnostics.
        formatter.write_str("HeartbeatRequest { redacted }")
    }
}

fn runtime_component(role: &str) -> &'static str {
    match role {
        "managed_runner" => "jobs-runner",
        "jobs_api" | "workflow_command_dispatcher" | "workflow_cleanup_dispatcher" => "jobs-api",
        _ => "jobs-workflows",
    }
}

impl HeartbeatRequest {
    fn validate(&self) -> ManagedCloudResult<()> {
        self.authority_binding.validate()?;
        self.dependency_probe.validate()?;
        require(
            self.version == 2
                && managed_cloud_route_id(&self.runtime_instance_id)
                && managed_cloud_route_id(&self.worker_id)
                && managed_cloud_base64url_32(&self.session_token)
                && managed_cloud_safe_integer(self.heartbeat_sequence, true)
                && managed_cloud_safe_integer(self.observed_head_revision, true)
                && managed_cloud_runtime_role(&self.role)
                && self.component_id == runtime_component(&self.role)
                && self.dependency_probe.role() == self.role
                && self
                    .acknowledged_latch_sha256
                    .as_deref()
                    .is_none_or(managed_cloud_hex64),
        )?;
        for hash in [
            &self.observed_transition_sha256,
            &self.activation_sha256,
            &self.manifest_sha256,
            &self.artifact_sha256,
            &self.migration_set_sha256,
            &self.config_schema_sha256,
            &self.protocol_set_sha256,
            &self.task_queue_sha256,
            &self.failure_converter_sha256,
            &self.dependency_evidence_sha256,
        ] {
            require(managed_cloud_hex64(hash))?;
        }
        if let Some(queue) = &self.queue_observation {
            require(
                self.role == "workflow_worker"
                    && managed_cloud_hex64(&queue.poller_identity_sha256)
                    && managed_cloud_safe_integer(queue.backlog_count, false)
                    && managed_cloud_safe_integer(queue.delivered_task_count, false),
            )?;
        }
        match (&self.boot_id, self.boot_generation) {
            (None, None) => {}
            (Some(id), Some(generation)) => require(
                self.role == "original_source_verifier"
                    && osv_identifier(id)
                    && managed_cloud_safe_integer(generation, true),
            )?,
            _ => return Err(ManagedCloudRegistryError::InvalidAuthority),
        }
        // Scalar enums are not the unapproved phase/health/probe eligibility matrix.
        // In particular, parsing a fitting digest or a Ready label grants no permission.
        require(managed_cloud_canonical_json(self)?.len() <= MAX_PROBE_BYTES)
    }
}

/// Ordinary HTTP JSON need not have canonical spacing or a final LF. Inspect
/// original bytes so duplicate/unknown fields, invalid UTF-8 and null boot fields reject.
/// Typed integer spellings match the accepted JSON.stringify writer, not every
/// lexical Number spelling that JavaScript can normalize (for example 1.0).
pub(crate) fn parse_heartbeat_request(bytes: &[u8]) -> ManagedCloudResult<HeartbeatRequest> {
    require(!bytes.is_empty() && bytes.len() <= MAX_PROBE_BYTES)?;
    let request: HeartbeatRequest =
        serde_json::from_slice(bytes).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    request.validate()?;
    Ok(request)
}

/// HTTP request peers used by the provisioning transactions. No request can
/// construct the process context or grant itself authority through these codecs.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaimRequest {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) grant_id: String,
    pub(crate) grant_token: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) worker_id: String,
    pub(crate) session_token: String,
    pub(crate) runtime_identity_sha256: String,
}

impl std::fmt::Debug for ClaimRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaimRequest { redacted }")
    }
}

pub(crate) fn parse_claim_request(bytes: &[u8]) -> ManagedCloudResult<ClaimRequest> {
    require(!bytes.is_empty() && bytes.len() <= MAX_PROBE_BYTES)?;
    let value: ClaimRequest =
        serde_json::from_slice(bytes).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    value.authority_binding.validate()?;
    require(
        value.version == 2
            && managed_cloud_route_id(&value.grant_id)
            && managed_cloud_route_id(&value.runtime_instance_id)
            && managed_cloud_route_id(&value.worker_id)
            && managed_cloud_base64url_32(&value.grant_token)
            && managed_cloud_base64url_32(&value.session_token)
            && managed_cloud_hex64(&value.runtime_identity_sha256),
    )?;
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaimResponse {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) serving_latch_sha256: Option<String>,
    pub(crate) grant_id: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) runtime_identity_sha256: String,
    pub(crate) worker_id: String,
    pub(crate) scope: super::ManagedCloudScope,
    pub(crate) activation_sha256: String,
    pub(crate) activation_expires_at_ms: i64,
    pub(crate) manifest_sha256: String,
    pub(crate) component_id: String,
    pub(crate) role: String,
    pub(crate) head_revision: i64,
    pub(crate) transition_sha256: String,
    pub(crate) artifact_sha256: String,
    pub(crate) config_schema_sha256: String,
    pub(crate) migration_set_sha256: String,
    pub(crate) protocol_set_sha256: String,
    pub(crate) task_queue_sha256: String,
    pub(crate) failure_converter_sha256: String,
    pub(crate) dependency_evidence_sha256: String,
    pub(crate) instance_epoch: i64,
    pub(crate) next_heartbeat_sequence: i64,
    pub(crate) claimed_at_ms: i64,
    pub(crate) replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HeartbeatResponse {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) runtime_instance_id: String,
    pub(crate) worker_id: String,
    pub(crate) heartbeat_sequence: i64,
    pub(crate) observed_head_revision: i64,
    pub(crate) observed_transition_sha256: String,
    pub(crate) activation_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) component_id: String,
    pub(crate) role: String,
    pub(crate) artifact_sha256: String,
    pub(crate) migration_set_sha256: String,
    pub(crate) config_schema_sha256: String,
    pub(crate) protocol_set_sha256: String,
    pub(crate) task_queue_sha256: String,
    pub(crate) failure_converter_sha256: String,
    pub(crate) dependency_evidence_sha256: String,
    pub(crate) lifecycle_state: LifecycleState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) acknowledged_latch_sha256: Option<String>,
    pub(crate) dependency_probe: DependencyProbe,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) queue_observation: Option<QueueObservation>,
    pub(crate) health_state: HeartbeatHealthState,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) reason_code: Option<HeartbeatReasonCode>,
    pub(crate) instance_epoch: i64,
    pub(crate) heartbeat_at_ms: i64,
    pub(crate) replayed: bool,
    pub(crate) observation_sha256: String,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) serving_latch_sha256: Option<String>,
}

impl HeartbeatResponse {
    pub(super) fn from_request(
        request: &HeartbeatRequest,
        epoch: i64,
        at_ms: i64,
        latch: Option<String>,
        digest: String,
        replayed: bool,
    ) -> Self {
        Self {
            version: 2,
            authority_binding: request.authority_binding.clone(),
            runtime_instance_id: request.runtime_instance_id.clone(),
            worker_id: request.worker_id.clone(),
            heartbeat_sequence: request.heartbeat_sequence,
            observed_head_revision: request.observed_head_revision,
            observed_transition_sha256: request.observed_transition_sha256.clone(),
            activation_sha256: request.activation_sha256.clone(),
            manifest_sha256: request.manifest_sha256.clone(),
            component_id: request.component_id.clone(),
            role: request.role.clone(),
            artifact_sha256: request.artifact_sha256.clone(),
            migration_set_sha256: request.migration_set_sha256.clone(),
            config_schema_sha256: request.config_schema_sha256.clone(),
            protocol_set_sha256: request.protocol_set_sha256.clone(),
            task_queue_sha256: request.task_queue_sha256.clone(),
            failure_converter_sha256: request.failure_converter_sha256.clone(),
            dependency_evidence_sha256: request.dependency_evidence_sha256.clone(),
            lifecycle_state: request.lifecycle_state,
            acknowledged_latch_sha256: request.acknowledged_latch_sha256.clone(),
            dependency_probe: request.dependency_probe.clone(),
            queue_observation: request.queue_observation.clone(),
            health_state: request.health_state,
            reason_code: request.reason_code,
            instance_epoch: epoch,
            heartbeat_at_ms: at_ms,
            replayed,
            observation_sha256: digest,
            serving_latch_sha256: latch,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BootRegistrationRequest {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) session_token: String,
    pub(crate) registration_request_id: String,
    pub(crate) boot_id: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) instance_epoch: i64,
    pub(crate) worker_id: String,
    pub(crate) runtime_identity_sha256: String,
    pub(crate) activation_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) component_id: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BootRegistrationResponse {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) registration_request_id: String,
    pub(crate) boot_id: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) instance_epoch: i64,
    pub(crate) worker_id: String,
    pub(crate) runtime_identity_sha256: String,
    pub(crate) activation_sha256: String,
    pub(crate) manifest_sha256: String,
    pub(crate) component_id: String,
    pub(crate) role: String,
    pub(crate) boot_generation: i64,
    pub(crate) registered_at_ms: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BootStartupRequest {
    pub(crate) version: u8,
    pub(crate) authority_binding: AuthorityBinding,
    pub(crate) session_token: String,
    pub(crate) startup_request_id: String,
    pub(crate) boot_id: String,
    pub(crate) runtime_instance_id: String,
    pub(crate) instance_epoch: i64,
    pub(crate) boot_generation: i64,
    pub(crate) worker_id: String,
    pub(crate) prior_heartbeat_sequence: i64,
    pub(crate) dependency_probe: DependencyProbe,
}

impl std::fmt::Debug for BootRegistrationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BootRegistrationRequest { redacted }")
    }
}
impl std::fmt::Debug for BootStartupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BootStartupRequest { redacted }")
    }
}

pub(crate) fn parse_boot_registration_request(
    bytes: &[u8],
) -> ManagedCloudResult<BootRegistrationRequest> {
    require(!bytes.is_empty() && bytes.len() <= MAX_PROBE_BYTES)?;
    let value: BootRegistrationRequest =
        serde_json::from_slice(bytes).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    value.authority_binding.validate()?;
    require(
        value.version == 2
            && osv_identifier(&value.registration_request_id)
            && osv_identifier(&value.boot_id)
            && managed_cloud_route_id(&value.runtime_instance_id)
            && managed_cloud_route_id(&value.worker_id)
            && managed_cloud_safe_integer(value.instance_epoch, true)
            && managed_cloud_base64url_32(&value.session_token)
            && managed_cloud_hex64(&value.runtime_identity_sha256)
            && managed_cloud_hex64(&value.activation_sha256)
            && managed_cloud_hex64(&value.manifest_sha256)
            && value.role == "original_source_verifier"
            && value.component_id == "jobs-workflows",
    )?;
    Ok(value)
}

pub(crate) fn parse_boot_startup_request(bytes: &[u8]) -> ManagedCloudResult<BootStartupRequest> {
    require(!bytes.is_empty() && bytes.len() <= MAX_PROBE_BYTES)?;
    let value: BootStartupRequest =
        serde_json::from_slice(bytes).map_err(|_| ManagedCloudRegistryError::InvalidRequest)?;
    value.authority_binding.validate()?;
    value.dependency_probe.validate()?;
    require(
        value.version == 2
            && osv_identifier(&value.startup_request_id)
            && osv_identifier(&value.boot_id)
            && managed_cloud_route_id(&value.runtime_instance_id)
            && managed_cloud_route_id(&value.worker_id)
            && managed_cloud_safe_integer(value.instance_epoch, true)
            && managed_cloud_safe_integer(value.boot_generation, true)
            && managed_cloud_safe_integer(value.prior_heartbeat_sequence, true)
            && value.prior_heartbeat_sequence < super::MANAGED_CLOUD_MAX_SAFE_INTEGER
            && managed_cloud_base64url_32(&value.session_token)
            && value.dependency_probe.role() == "original_source_verifier",
    )?;
    Ok(value)
}

fn canonical_observation_bytes(
    grant_id: &str,
    heartbeat: &HeartbeatRequest,
    heartbeat_at_ms: i64,
    instance_epoch: i64,
    serving_latch_sha256: Option<&str>,
) -> ManagedCloudResult<Vec<u8>> {
    heartbeat.validate()?;
    require(
        managed_cloud_route_id(grant_id)
            && managed_cloud_safe_integer(heartbeat_at_ms, true)
            && managed_cloud_safe_integer(instance_epoch, true)
            && serving_latch_sha256.is_none_or(managed_cloud_hex64),
    )?;
    // This Value is produced ONLY from the validated closed typed request.
    // Remove exactly the secret, retaining the optional request-only OSV pair.
    let mut nonsecret =
        serde_json::to_value(heartbeat).map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    require(
        nonsecret
            .as_object_mut()
            .and_then(|row| row.remove("sessionToken"))
            .is_some(),
    )?;
    let canonical = managed_cloud_canonical_json(&serde_json::json!({
        "grantId": grant_id,
        "heartbeat": nonsecret,
        "heartbeatAtMs": heartbeat_at_ms,
        "instanceEpoch": instance_epoch,
        "servingLatchSha256": serving_latch_sha256,
    }))?;
    require(canonical.len() <= MAX_PROBE_BYTES)?;
    Ok(canonical)
}

/// Byte coherence only: no atomic storage, freshness, boot or serving proof.
pub(crate) fn observation_sha256_v2(
    grant_id: &str,
    heartbeat: &HeartbeatRequest,
    heartbeat_at_ms: i64,
    instance_epoch: i64,
    serving_latch_sha256: Option<&str>,
) -> ManagedCloudResult<String> {
    let canonical = canonical_observation_bytes(
        grant_id,
        heartbeat,
        heartbeat_at_ms,
        instance_epoch,
        serving_latch_sha256,
    )?;
    let mut preimage = OBSERVATION_DOMAIN.to_vec();
    preimage.extend_from_slice(&canonical);
    Ok(managed_cloud_sha256(&preimage))
}

pub(crate) fn derive_runtime_session_token_v2(
    grant_token: &str,
    grant_id: &str,
    runtime_instance_id: &str,
    binding: &AuthorityBinding,
) -> ManagedCloudResult<String> {
    require(
        managed_cloud_base64url_32(grant_token)
            && managed_cloud_route_id(grant_id)
            && managed_cloud_route_id(runtime_instance_id),
    )?;
    let canonical = canonical_binding_bytes(binding)?;
    let key = managed_cloud_decode_exact(grant_token, 32)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key)
        .map_err(|_| ManagedCloudRegistryError::InvalidAuthority)?;
    mac.update(SESSION_DOMAIN);
    mac.update(grant_id.as_bytes());
    mac.update(&[0]);
    mac.update(runtime_instance_id.as_bytes());
    mac.update(&[0]);
    mac.update(&canonical);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

pub(crate) struct RuntimeDependencyBinding<'a> {
    pub(crate) role: &'a str,
    pub(crate) activation_sha256: &'a str,
    pub(crate) manifest_sha256: &'a str,
    pub(crate) component_id: &'a str,
    pub(crate) artifact_sha256: &'a str,
    pub(crate) task_queue_sha256: &'a str,
    pub(crate) failure_converter_sha256: &'a str,
}

pub(crate) fn dependency_evidence_sha256_v2(
    fields: &RuntimeDependencyBinding<'_>,
    binding: &AuthorityBinding,
) -> ManagedCloudResult<String> {
    let expected_component = runtime_component(fields.role);
    require(managed_cloud_runtime_role(fields.role) && fields.component_id == expected_component)?;
    for value in [
        fields.activation_sha256,
        fields.manifest_sha256,
        fields.artifact_sha256,
        fields.task_queue_sha256,
        fields.failure_converter_sha256,
    ] {
        require(managed_cloud_hex64(value))?;
    }
    let canonical = canonical_binding_bytes(binding)?;
    let mut bytes = DEPENDENCY_DOMAIN.to_vec();
    for value in [
        fields.role,
        fields.activation_sha256,
        fields.manifest_sha256,
        fields.component_id,
        fields.artifact_sha256,
        fields.task_queue_sha256,
        fields.failure_converter_sha256,
    ] {
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
    bytes.extend_from_slice(&canonical);
    bytes.push(0);
    Ok(managed_cloud_sha256(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const TOKEN: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    const PRODUCTION: &str = concat!(
        "{\"authorityKind\":\"provisioning_v4\",\"contextSha256\":null,",
        "\"purpose\":\"production\",\"queueGenerationId\":\"qqqqqqqqqqqqqqqqqqqq\"}\n"
    );
    const SYNTHETIC: &str = concat!(
        "{\"authorityKind\":\"provisioning_v4\",\"contextSha256\":\"",
        "cccccccccccccccc",
        "cccccccccccccccc",
        "cccccccccccccccc",
        "cccccccccccccccc",
        "\",\"purpose\":\"nonproduction_synthetic\",",
        "\"queueGenerationId\":\"qqqqqqqqqqqqqqqqqqqq\"}\n"
    );

    fn raw(value: &Value) -> Vec<u8> {
        managed_cloud_canonical_json(value).expect("canonical unit fixture")
    }

    fn bindings() -> [AuthorityBinding; 2] {
        [
            managed_cloud_parse_canonical(PRODUCTION.as_bytes()).expect("production unit binding"),
            managed_cloud_parse_canonical(SYNTHETIC.as_bytes()).expect("synthetic unit binding"),
        ]
    }

    fn fields(hashes: &[String; 5]) -> RuntimeDependencyBinding<'_> {
        RuntimeDependencyBinding {
            role: "managed_runner",
            activation_sha256: &hashes[0],
            manifest_sha256: &hashes[1],
            component_id: "jobs-runner",
            artifact_sha256: &hashes[2],
            task_queue_sha256: &hashes[3],
            failure_converter_sha256: &hashes[4],
        }
    }

    fn namespace() -> Value {
        json!({
            "endpointId": "e".repeat(20), "transport": "tls",
            "authority": "temporal.internal:7233", "namespace": "phase648",
            "namespaceId": "namespace-id", "state": "registered"
        })
    }

    fn gateway() -> Value {
        json!({
            "endpointId": "e".repeat(20), "transport": "https",
            "authority": "gateway.internal:443", "runtimeInstanceId": "r".repeat(20),
            "instanceEpoch": 1, "headRevision": 1, "transitionSha256": "a".repeat(64),
            "lifecycleState": "parked", "observationSha256": "b".repeat(64)
        })
    }

    fn boot() -> Value {
        json!({
            "bootId": "b".repeat(20), "bootGeneration": 1,
            "startupRequestId": "s".repeat(20), "parkedHeartbeatSequence": 2
        })
    }

    fn volume() -> Value {
        json!({
            "volumeId": TOKEN, "enrollmentEpoch": 1, "processInstanceId": TOKEN,
            "runtimeGrantId": "runtime:grant+unit.648", "runtimeSha256": "a".repeat(64),
            "storageAttestationGeneration": 1, "storageAttestationSha256": "b".repeat(64)
        })
    }

    fn probes() -> Vec<Value> {
        vec![
            json!({"version": 1, "kind": "jobs_api", "listenerState": "listening",
                "serverState": "running"}),
            json!({"version": 1, "kind": "workflow_command_dispatcher", "loopState": "parked",
                "controlProgress": "fresh", "gatewayControl": gateway()}),
            json!({"version": 1, "kind": "workflow_cleanup_dispatcher", "loopState": "parked",
                "controlProgress": "fresh", "gatewayControl": gateway()}),
            json!({"version": 1, "kind": "workflow_gateway", "listenerState": "listening",
                "temporalNamespace": namespace()}),
            json!({"version": 1, "kind": "workflow_worker", "workerState": "running",
                "temporalNamespace": namespace()}),
            json!({"version": 1, "kind": "discovery_worker", "loopState": "parked",
                "controlProgress": "fresh"}),
            json!({"version": 1, "kind": "global_discovery_worker", "loopState": "parked",
                "controlProgress": "fresh"}),
            json!({"version": 1, "kind": "original_source_verifier", "loopState": "parked",
                "controlProgress": "fresh", "bootStartup": boot()}),
            json!({"version": 1, "kind": "managed_runner", "listenerState": "listening",
                "storageRecoveryComplete": true, "volumeBinding": volume()}),
        ]
    }

    // Exact synthetic fixture shared with the independently checked Node/Python/OpenSSL vectors.
    fn heartbeat_fixture() -> Value {
        json!({
            "version": 2, "authorityBinding": serde_json::from_str::<Value>(PRODUCTION).unwrap(),
            "lifecycleState": "parked", "acknowledgedLatchSha256": null,
            "dependencyProbe": {"version": 1, "kind": "workflow_worker", "workerState": "running",
                "temporalNamespace": {"endpointId": "e".repeat(20), "transport": "tls",
                    "authority": "temporal.example.test:7233", "namespace": "synthetic-namespace",
                    "namespaceId": "synthetic-namespace-id", "state": "registered"}},
            "queueObservation": {"pollerIdentitySha256": "5".repeat(64), "backlogCount": 0,
                "deliveredTaskCount": 0},
            "runtimeInstanceId": "r".repeat(20), "workerId": "w".repeat(20), "sessionToken": TOKEN,
            "heartbeatSequence": 1, "observedHeadRevision": 1, "observedTransitionSha256": "c".repeat(64),
            "activationSha256": "a".repeat(64), "manifestSha256": "b".repeat(64),
            "componentId": "jobs-workflows", "role": "workflow_worker", "artifactSha256": "d".repeat(64),
            "migrationSetSha256": "f".repeat(64), "configSchemaSha256": "e".repeat(64),
            "protocolSetSha256": "0".repeat(64), "taskQueueSha256": "2".repeat(64),
            "failureConverterSha256": "3".repeat(64), "dependencyEvidenceSha256": "4".repeat(64),
            "healthState": "degraded", "reasonCode": "startup"
        })
    }

    fn osv_heartbeat_fixture() -> Value {
        let mut value = heartbeat_fixture();
        value["authorityBinding"] = serde_json::from_str(SYNTHETIC).unwrap();
        value["role"] = json!("original_source_verifier");
        value["heartbeatSequence"] = json!(3);
        value["queueObservation"] = Value::Null;
        value["bootId"] = json!("boot-fixture:/.-_identifier");
        value["bootGeneration"] = json!(2);
        value["dependencyProbe"] = json!({"version": 1, "kind": "original_source_verifier",
            "loopState": "parked", "controlProgress": "fresh", "bootStartup": {
                "bootId": "boot-fixture:/.-_identifier", "bootGeneration": 2,
                "startupRequestId": "startup-fixture:/.-_identifier", "parkedHeartbeatSequence": 2}});
        value
    }

    fn http(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).expect("unit HTTP JSON, not canonical document authority")
    }

    #[test]
    fn heartbeat_http_reader_accepts_closed_shapes_without_canonical_spacing() {
        for (value, count) in [(heartbeat_fixture(), 25), (osv_heartbeat_fixture(), 27)] {
            assert_eq!(value.as_object().unwrap().len(), count);
            let compact = parse_heartbeat_request(&http(&value)).unwrap();
            assert_eq!(format!("{compact:?}"), "HeartbeatRequest { redacted }");
            let pretty = format!(" \n{}\n\t", serde_json::to_string_pretty(&value).unwrap());
            assert_eq!(parse_heartbeat_request(pretty.as_bytes()).unwrap(), compact);
            assert_eq!(serde_json::to_value(compact).unwrap(), value);
        }
    }

    #[test]
    fn heartbeat_http_reader_requires_every_key_and_rejects_unknown_and_duplicate_keys() {
        for value in [heartbeat_fixture(), osv_heartbeat_fixture()] {
            let object = value.as_object().unwrap();
            let body = String::from_utf8(http(&value)).unwrap();
            for (key, field) in object {
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove(key);
                assert!(
                    parse_heartbeat_request(&http(&missing)).is_err(),
                    "missing {key}"
                );
                let duplicate = format!(
                    "{},{key}:{field}}}",
                    &body[..body.len() - 1],
                    key = serde_json::to_string(key).unwrap(),
                    field = serde_json::to_string(field).unwrap()
                );
                assert!(
                    parse_heartbeat_request(duplicate.as_bytes()).is_err(),
                    "duplicate {key}"
                );
            }
            for key in [
                "unknown",
                "instanceEpoch",
                "replayed",
                "observationSha256",
                "servingLatchSha256",
            ] {
                let mut extra = value.clone();
                extra[key] = Value::Null;
                assert!(parse_heartbeat_request(&http(&extra)).is_err());
            }
            let probe = serde_json::to_string(&value["dependencyProbe"]).unwrap();
            assert_eq!(body.matches(probe.as_str()).count(), 1);
            let duplicate_probe = format!("{},\"version\":1}}", &probe[..probe.len() - 1]);
            let duplicate_body = body.replacen(&probe, &duplicate_probe, 1);
            assert_ne!(duplicate_body, body);
            assert!(parse_heartbeat_request(duplicate_body.as_bytes()).is_err());
            for key in ["authorityBinding", "dependencyProbe", "queueObservation"] {
                if value[key].is_object() {
                    let mut extra = value.clone();
                    extra[key]["unknown"] = Value::Null;
                    assert!(parse_heartbeat_request(&http(&extra)).is_err());
                }
            }
        }
        assert!(parse_heartbeat_request(&[0xff]).is_err());
        assert!(parse_heartbeat_request(&vec![b' '; MAX_PROBE_BYTES + 1]).is_err());
    }

    #[test]
    fn heartbeat_http_reader_rejects_role_component_probe_and_queue_mismatch() {
        for (key, replacement) in [
            ("role", json!("jobs_api")),
            ("role", json!("unknown_worker")),
            ("componentId", json!("jobs-api")),
            ("dependencyProbe", probes()[0].clone()),
            (
                "queueObservation",
                json!({"pollerIdentitySha256": "5".repeat(64),
                "backlogCount": 0, "deliveredTaskCount": 0, "unknown": 0}),
            ),
        ] {
            let mut value = heartbeat_fixture();
            value[key] = replacement;
            assert!(parse_heartbeat_request(&http(&value)).is_err());
        }
        let mut other = heartbeat_fixture();
        other["role"] = json!("jobs_api");
        other["componentId"] = json!("jobs-api");
        other["dependencyProbe"] = probes()[0].clone();
        assert!(parse_heartbeat_request(&http(&other)).is_err());
        other["queueObservation"] = Value::Null;
        assert!(parse_heartbeat_request(&http(&other)).is_ok());
        let mut unavailable = heartbeat_fixture();
        unavailable["queueObservation"] = Value::Null;
        assert!(
            parse_heartbeat_request(&http(&unavailable)).is_ok(),
            "shape-only negative worker evidence"
        );
        for probe in probes() {
            let mut value = heartbeat_fixture();
            value["role"] = probe["kind"].clone();
            value["componentId"] = json!(match probe["kind"].as_str().unwrap() {
                "managed_runner" => "jobs-runner",
                "jobs_api" | "workflow_command_dispatcher" | "workflow_cleanup_dispatcher" =>
                    "jobs-api",
                _ => "jobs-workflows",
            });
            value["dependencyProbe"] = probe;
            value["queueObservation"] = Value::Null;
            assert!(parse_heartbeat_request(&http(&value)).is_ok());
        }
    }

    #[test]
    fn heartbeat_http_reader_distinguishes_absent_from_null_osv_boot_fences() {
        let full = osv_heartbeat_fixture();
        for (id, generation) in [
            (Value::Null, json!(2)),
            (json!("boot-fixture:/.-_identifier"), Value::Null),
            (Value::Null, Value::Null),
            (json!("short"), json!(2)),
            (json!("boot-fixture:/.-_identifier"), json!(0)),
        ] {
            let mut value = full.clone();
            value["bootId"] = id;
            value["bootGeneration"] = generation;
            assert!(parse_heartbeat_request(&http(&value)).is_err());
        }
        let mut absent = full.clone();
        absent.as_object_mut().unwrap().remove("bootId");
        assert!(parse_heartbeat_request(&http(&absent)).is_err());
        absent.as_object_mut().unwrap().remove("bootGeneration");
        assert!(parse_heartbeat_request(&http(&absent)).is_ok());
        let mut non_osv = heartbeat_fixture();
        non_osv["bootId"] = full["bootId"].clone();
        non_osv["bootGeneration"] = full["bootGeneration"].clone();
        assert!(parse_heartbeat_request(&http(&non_osv)).is_err());
    }

    #[test]
    fn heartbeat_http_reader_enforces_scalar_and_nullable_bounds() {
        for (key, replacement) in [
            ("version", json!(1)),
            ("version", json!("2")),
            ("runtimeInstanceId", json!("r".repeat(19))),
            ("workerId", json!("w".repeat(129))),
            ("sessionToken", json!(format!("{TOKEN}="))),
            ("heartbeatSequence", json!(0)),
            ("heartbeatSequence", json!(9007199254740992_i64)),
            ("observedHeadRevision", json!(1.5)),
            ("acknowledgedLatchSha256", json!("A".repeat(64))),
            ("healthState", json!("unknown")),
            ("reasonCode", json!("unknown")),
            ("lifecycleState", json!("ready")),
        ] {
            let mut value = heartbeat_fixture();
            value[key] = replacement;
            assert!(parse_heartbeat_request(&http(&value)).is_err());
        }
        for key in [
            "observedTransitionSha256",
            "activationSha256",
            "manifestSha256",
            "artifactSha256",
            "migrationSetSha256",
            "configSchemaSha256",
            "protocolSetSha256",
            "taskQueueSha256",
            "failureConverterSha256",
            "dependencyEvidenceSha256",
        ] {
            let mut value = heartbeat_fixture();
            value[key] = json!("G".repeat(64));
            assert!(parse_heartbeat_request(&http(&value)).is_err());
        }
        for key in ["backlogCount", "deliveredTaskCount"] {
            for replacement in [json!(-1), json!(1.5), json!(9007199254740992_i64)] {
                let mut value = heartbeat_fixture();
                value["queueObservation"][key] = replacement;
                assert!(parse_heartbeat_request(&http(&value)).is_err());
            }
        }
        let body = String::from_utf8(http(&heartbeat_fixture())).unwrap();
        for spelling in ["-0", "1.0", "1e0"] {
            const SITE: &str = "\"backlogCount\":0";
            assert_eq!(body.matches(SITE).count(), 1);
            let changed = body.replacen(SITE, &format!("\"backlogCount\":{spelling}"), 1);
            assert_ne!(changed, body);
            assert!(parse_heartbeat_request(changed.as_bytes()).is_err());
        }
        let mut value = heartbeat_fixture();
        value["reasonCode"] = Value::Null;
        assert!(
            parse_heartbeat_request(&http(&value)).is_ok(),
            "no lifecycle/health permission is inferred"
        );
        value["sessionToken"] = json!("private-session-do-not-log");
        let error = parse_heartbeat_request(&http(&value)).unwrap_err();
        assert_eq!(error.to_string(), "invalid managed cloud authority");
    }

    #[test]
    fn production_worker_observation_matches_independent_literal_vector() {
        let heartbeat = parse_heartbeat_request(&http(&heartbeat_fixture())).unwrap();
        let canonical =
            canonical_observation_bytes(&"g".repeat(20), &heartbeat, 101, 2, None).unwrap();
        assert_eq!(canonical.len(), 1877);
        assert_eq!(OBSERVATION_DOMAIN.len() + canonical.len(), 1925);
        assert_eq!(
            observation_sha256_v2(&"g".repeat(20), &heartbeat, 101, 2, None).unwrap(),
            "c823c91681a40ae99e1136bebb4f195a86ca9910fbb1cfeff4a50990832dee52"
        );
        let envelope: Value = serde_json::from_slice(&canonical).unwrap();
        assert_eq!(envelope.as_object().unwrap().len(), 5);
        assert_eq!(envelope["heartbeat"].as_object().unwrap().len(), 24);
        assert!(envelope["heartbeat"].get("sessionToken").is_none());
        assert!(envelope["heartbeat"].get("bootId").is_none());
        assert_eq!(canonical.last(), Some(&b'\n'));
    }

    #[test]
    fn synthetic_osv_observation_matches_independent_literal_vector() {
        let heartbeat = parse_heartbeat_request(&http(&osv_heartbeat_fixture())).unwrap();
        let latch = "6".repeat(64);
        let canonical =
            canonical_observation_bytes(&"g".repeat(20), &heartbeat, 102, 3, Some(&latch)).unwrap();
        assert_eq!(canonical.len(), 1929);
        assert_eq!(OBSERVATION_DOMAIN.len() + canonical.len(), 1977);
        assert_eq!(
            observation_sha256_v2(&"g".repeat(20), &heartbeat, 102, 3, Some(&latch)).unwrap(),
            "2dc928ef6bb57fec1a74bc1788b771fc455b06320bf10800572d4edd33dff3f4"
        );
        let envelope: Value = serde_json::from_slice(&canonical).unwrap();
        assert_eq!(envelope["heartbeat"].as_object().unwrap().len(), 26);
        assert_eq!(
            envelope["heartbeat"]["bootId"],
            "boot-fixture:/.-_identifier"
        );
        assert_eq!(envelope["heartbeat"]["bootGeneration"], 2);
    }

    #[test]
    fn observation_digest_binds_all_nonsecret_fields_and_acceptance_envelope() {
        let value = heartbeat_fixture();
        let heartbeat = parse_heartbeat_request(&http(&value)).unwrap();
        let baseline = "c823c91681a40ae99e1136bebb4f195a86ca9910fbb1cfeff4a50990832dee52";
        for (grant, time, epoch, latch) in [
            ("h".repeat(20), 101, 2, None),
            ("g".repeat(20), 102, 2, None),
            ("g".repeat(20), 101, 3, None),
            ("g".repeat(20), 101, 2, Some("6".repeat(64))),
        ] {
            assert_ne!(
                observation_sha256_v2(&grant, &heartbeat, time, epoch, latch.as_deref()).unwrap(),
                baseline
            );
        }
        let mut changes = vec![
            (
                "authorityBinding",
                serde_json::from_str::<Value>(SYNTHETIC).unwrap(),
            ),
            ("lifecycleState", json!("operational")),
            ("acknowledgedLatchSha256", json!("6".repeat(64))),
            ("dependencyProbe", {
                let mut probe = value["dependencyProbe"].clone();
                probe["workerState"] = json!("not_running");
                probe
            }),
            (
                "queueObservation",
                json!({"pollerIdentitySha256":"6".repeat(64),"backlogCount":1,"deliveredTaskCount":1}),
            ),
            ("runtimeInstanceId", json!("s".repeat(20))),
            ("workerId", json!("x".repeat(20))),
            ("heartbeatSequence", json!(2)),
            ("observedHeadRevision", json!(2)),
            ("healthState", json!("draining")),
            ("reasonCode", json!("probe_failed")),
        ];
        for key in [
            "observedTransitionSha256",
            "activationSha256",
            "manifestSha256",
            "artifactSha256",
            "migrationSetSha256",
            "configSchemaSha256",
            "protocolSetSha256",
            "taskQueueSha256",
            "failureConverterSha256",
            "dependencyEvidenceSha256",
        ] {
            changes.push((key, json!("7".repeat(64))));
        }
        for (key, replacement) in changes {
            let mut changed = value.clone();
            changed[key] = replacement;
            let parsed = parse_heartbeat_request(&http(&changed)).unwrap();
            assert_ne!(
                observation_sha256_v2(&"g".repeat(20), &parsed, 101, 2, None).unwrap(),
                baseline,
                "{key}"
            );
        }
        let mut changed = value;
        changed["role"] = json!("jobs_api");
        changed["componentId"] = json!("jobs-api");
        changed["dependencyProbe"] = probes()[0].clone();
        changed["queueObservation"] = Value::Null;
        let parsed = parse_heartbeat_request(&http(&changed)).unwrap();
        assert_ne!(
            observation_sha256_v2(&"g".repeat(20), &parsed, 101, 2, None).unwrap(),
            baseline
        );
    }

    #[test]
    fn observation_digest_retains_osv_pair_and_excludes_only_valid_session() {
        let mut heartbeat = parse_heartbeat_request(&http(&osv_heartbeat_fixture())).unwrap();
        let latch = "6".repeat(64);
        let baseline = "2dc928ef6bb57fec1a74bc1788b771fc455b06320bf10800572d4edd33dff3f4";
        heartbeat.session_token = "A".repeat(43);
        assert_eq!(
            observation_sha256_v2(&"g".repeat(20), &heartbeat, 102, 3, Some(&latch)).unwrap(),
            baseline
        );
        let mut changed = heartbeat.clone();
        changed.boot_id = Some("changed-boot:/.-_identifier".into());
        assert_ne!(
            observation_sha256_v2(&"g".repeat(20), &changed, 102, 3, Some(&latch)).unwrap(),
            baseline
        );
        let mut changed = heartbeat.clone();
        changed.boot_generation = Some(3);
        assert_ne!(
            observation_sha256_v2(&"g".repeat(20), &changed, 102, 3, Some(&latch)).unwrap(),
            baseline
        );
        heartbeat.boot_id = None;
        heartbeat.boot_generation = None;
        assert_ne!(
            observation_sha256_v2(&"g".repeat(20), &heartbeat, 102, 3, Some(&latch)).unwrap(),
            baseline
        );
    }

    #[test]
    fn observation_digest_revalidates_typed_request_and_rejects_invalid_envelope() {
        let heartbeat = parse_heartbeat_request(&http(&heartbeat_fixture())).unwrap();
        for (grant, time, epoch, latch) in [
            ("short".into(), 101, 2, None),
            ("g".repeat(20), 0, 2, None),
            ("g".repeat(20), 101, 9007199254740992_i64, None),
            ("g".repeat(20), 101, 2, Some("F".repeat(64))),
        ] {
            assert!(
                observation_sha256_v2(&grant, &heartbeat, time, epoch, latch.as_deref()).is_err()
            );
        }
        let mut changed = heartbeat.clone();
        changed.session_token = "invalid".into();
        assert!(observation_sha256_v2(&"g".repeat(20), &changed, 101, 2, None).is_err());
        let mut changed = heartbeat.clone();
        changed.role = "jobs_api".into();
        assert!(observation_sha256_v2(&"g".repeat(20), &changed, 101, 2, None).is_err());
        let mut changed = heartbeat;
        changed.queue_observation.as_mut().unwrap().backlog_count = -1;
        assert!(observation_sha256_v2(&"g".repeat(20), &changed, 101, 2, None).is_err());
    }

    #[test]
    fn binding_bytes_match_frozen_production_and_synthetic_vectors() {
        for (bytes, length, expected) in [
            (
                PRODUCTION,
                123,
                "595bf6c1579282c2e42bedcf08ac2bbfab7d6652c946815c0bb38bfb29cd61fa",
            ),
            (
                SYNTHETIC,
                198,
                "8a39573146585c87058aaab6c16c5b0f0a4e794f834aec0ad3623505fce87b93",
            ),
        ] {
            let binding = managed_cloud_parse_canonical(bytes.as_bytes()).expect("unit vector");
            assert_eq!(bytes.len(), length);
            assert_eq!(canonical_binding_bytes(&binding).unwrap(), bytes.as_bytes());
            assert_eq!(managed_cloud_sha256(bytes.as_bytes()), expected);
        }
    }

    #[test]
    fn session_tokens_match_frozen_independently_checked_vectors() {
        for (binding, expected) in bindings().iter().zip([
            "423aiuEFviphpEpXAYiEUoreTMN-4yIHljpIPQ2IMDA",
            "poSM_ub4veL4PZAkJEc7jtYVGiWrGBTcFIB1WBKYOJw",
        ]) {
            assert_eq!(
                derive_runtime_session_token_v2(TOKEN, &"g".repeat(20), &"r".repeat(20), binding)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn dependency_hashes_match_frozen_independently_checked_vectors() {
        let hashes = ["a", "b", "d", "e", "f"].map(|value| value.repeat(64));
        for (binding, expected) in bindings().iter().zip([
            "58a6d8b0775c4adbc6d95159902111c4c125698f76e93fc32ddb35473ca37188",
            "ebdc575fdaf1f453ad782fac8e29fad26617e0b69f3ed361a0aab607e72b0fc1",
        ]) {
            assert_eq!(
                dependency_evidence_sha256_v2(&fields(&hashes), binding).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn binding_requires_complete_closed_record() {
        let original: Value = serde_json::from_str(PRODUCTION).unwrap();
        for key in [
            "authorityKind",
            "contextSha256",
            "purpose",
            "queueGenerationId",
        ] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(key);
            assert!(
                managed_cloud_parse_canonical::<AuthorityBinding>(&raw(&value)).is_err(),
                "missing {key}"
            );
        }
        let mut value = original;
        value["productionAllowed"] = json!(true);
        assert!(managed_cloud_parse_canonical::<AuthorityBinding>(&raw(&value)).is_err());
        assert!(managed_cloud_parse_canonical::<AuthorityBinding>(
            PRODUCTION
                .replace(
                    "\"contextSha256\":null,",
                    "\"contextSha256\":null,\"contextSha256\":null,"
                )
                .as_bytes()
        )
        .is_err());
    }

    #[test]
    fn binding_rejects_noncanonical_or_invalid_unicode_bytes() {
        for bytes in [
            PRODUCTION.trim_end().as_bytes().to_vec(),
            format!("{PRODUCTION}\n").into_bytes(),
            PRODUCTION.replace("{", "{ ").into_bytes(),
            PRODUCTION
                .replace("production\"", "produc\\u0074ion\"")
                .into_bytes(),
            PRODUCTION.replace("qqqq", "\\ud800").into_bytes(),
            vec![0xff, 0xfe, b'\n'],
        ] {
            assert!(managed_cloud_parse_canonical::<AuthorityBinding>(&bytes).is_err());
        }
    }

    #[test]
    fn binding_enforces_purpose_context_and_route_limits() {
        let original: Value = serde_json::from_str(PRODUCTION).unwrap();
        for (key, replacement) in [
            ("authorityKind", json!("legacy_activation")),
            ("purpose", json!("synthetic")),
            ("purpose", json!("nonproduction_synthetic")),
            ("contextSha256", json!("c".repeat(64))),
            ("queueGenerationId", json!("q".repeat(19))),
            ("queueGenerationId", json!("q".repeat(129))),
            ("queueGenerationId", json!("queue.with.dots.is.invalid")),
        ] {
            let mut value = original.clone();
            value[key] = replacement;
            assert!(
                managed_cloud_parse_canonical::<AuthorityBinding>(&raw(&value))
                    .and_then(|binding| canonical_binding_bytes(&binding))
                    .is_err(),
                "{key}"
            );
        }
        for length in [20, 128] {
            let mut value = original.clone();
            value["queueGenerationId"] = json!("q".repeat(length));
            assert!(
                managed_cloud_parse_canonical::<AuthorityBinding>(&raw(&value))
                    .and_then(|binding| canonical_binding_bytes(&binding))
                    .is_ok()
            );
        }
        let original: Value = serde_json::from_str(SYNTHETIC).unwrap();
        for context in [
            json!(null),
            json!("C".repeat(64)),
            json!("c".repeat(63)),
            json!(1),
        ] {
            let mut value = original.clone();
            value["contextSha256"] = context;
            assert!(
                managed_cloud_parse_canonical::<AuthorityBinding>(&raw(&value))
                    .and_then(|binding| canonical_binding_bytes(&binding))
                    .is_err()
            );
        }
    }

    #[test]
    fn heartbeat_reader_refuses_empty_and_oversized_input() {
        assert!(parse_heartbeat_request(&[]).is_err());
        assert!(parse_heartbeat_request(&vec![b' '; MAX_PROBE_BYTES + 1]).is_err());
    }

    #[test]
    fn session_rejects_noncanonical_token_or_identity() {
        let binding = &bindings()[0];
        for token in [
            format!("{TOKEN}="),
            format!("{TOKEN}\n"),
            format!("{}+", &TOKEN[..42]),
            "A".repeat(42),
            format!("{}9", &TOKEN[..42]),
        ] {
            assert!(derive_runtime_session_token_v2(
                &token,
                &"g".repeat(20),
                &"r".repeat(20),
                binding
            )
            .is_err());
        }
        for id in ["g".repeat(19), "g".repeat(129), "g".repeat(19) + ":"] {
            assert!(derive_runtime_session_token_v2(TOKEN, &id, &"r".repeat(20), binding).is_err());
            assert!(derive_runtime_session_token_v2(TOKEN, &"g".repeat(20), &id, binding).is_err());
        }
    }

    #[test]
    fn session_and_dependency_revalidate_constructed_binding() {
        let mut binding = bindings()[0].clone();
        binding.context_sha256 = Some("c".repeat(64));
        let hashes = ["a", "b", "d", "e", "f"].map(|value| value.repeat(64));
        assert!(canonical_binding_bytes(&binding).is_err());
        assert!(
            derive_runtime_session_token_v2(TOKEN, &"g".repeat(20), &"r".repeat(20), &binding)
                .is_err()
        );
        assert!(dependency_evidence_sha256_v2(&fields(&hashes), &binding).is_err());
    }

    #[test]
    fn dependency_hash_rejects_unknown_role_and_invalid_fields() {
        let hashes = ["a", "b", "d", "e", "f"].map(|value| value.repeat(64));
        let binding = &bindings()[0];
        let mut input = fields(&hashes);
        input.role = "jobs_portal";
        assert!(dependency_evidence_sha256_v2(&input, binding).is_err());
        input.role = "managed_runner";
        input.component_id = "jobs-workflows";
        assert!(dependency_evidence_sha256_v2(&input, binding).is_err());
        input.component_id = "jobs-runner\0hidden";
        assert!(dependency_evidence_sha256_v2(&input, binding).is_err());
        input.component_id = "jobs-runner";
        let uppercase = "A".repeat(64);
        input.artifact_sha256 = &uppercase;
        assert!(dependency_evidence_sha256_v2(&input, binding).is_err());
    }

    #[test]
    fn probes_roundtrip_all_nine_closed_role_arms() {
        let values = probes();
        assert_eq!(values.len(), 9);
        for value in values {
            let bytes = raw(&value);
            let probe = parse_dependency_probe(&bytes).expect("unit role probe");
            assert_eq!(probe.role(), value["kind"].as_str().unwrap());
            assert_eq!(canonical_probe_bytes(&probe).unwrap(), bytes);
        }
    }

    #[test]
    fn nullable_probe_reference_must_be_explicit() {
        for (index, key) in [
            (1, "gatewayControl"),
            (2, "gatewayControl"),
            (3, "temporalNamespace"),
            (4, "temporalNamespace"),
            (7, "bootStartup"),
            (8, "volumeBinding"),
        ] {
            let mut value = probes()[index].clone();
            value[key] = Value::Null;
            assert!(parse_dependency_probe(&raw(&value)).is_ok());
            value.as_object_mut().unwrap().remove(key);
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
    }

    #[test]
    fn probe_rejects_unknown_duplicate_and_wrong_role_fields() {
        for mut value in probes() {
            value["healthy"] = json!(true);
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
        let bytes = raw(&probes()[0]);
        let duplicate = String::from_utf8(bytes)
            .unwrap()
            .replace("\"version\":1", "\"version\":1,\"version\":1");
        assert!(parse_dependency_probe(duplicate.as_bytes()).is_err());
        let mut wrong = probes()[3].clone();
        wrong["kind"] = json!("jobs_api");
        assert!(parse_dependency_probe(&raw(&wrong)).is_err());
        wrong["kind"] = json!("unknown_role");
        assert!(parse_dependency_probe(&raw(&wrong)).is_err());
    }

    #[test]
    fn probe_reader_rejects_invalid_versions_and_numbers() {
        for version in ["0", "2", "1.0", "-1", "9007199254740992"] {
            let bytes = String::from_utf8(raw(&probes()[0]))
                .unwrap()
                .replace("\"version\":1", &format!("\"version\":{version}"));
            assert!(parse_dependency_probe(bytes.as_bytes()).is_err());
        }
        for number in ["0", "-1", "1.5", "9007199254740992"] {
            let bytes = String::from_utf8(raw(&probes()[1]))
                .unwrap()
                .replace("\"headRevision\":1", &format!("\"headRevision\":{number}"));
            assert!(parse_dependency_probe(bytes.as_bytes()).is_err());
        }
    }

    #[test]
    fn stopped_control_loop_requires_stale_progress() {
        for index in [1, 2, 5, 6, 7] {
            let mut value = probes()[index].clone();
            value["loopState"] = json!("stopped");
            assert!(parse_dependency_probe(&raw(&value)).is_err());
            value["controlProgress"] = json!("stale");
            assert!(parse_dependency_probe(&raw(&value)).is_ok());
        }
    }

    #[test]
    fn probe_reader_preserves_truthful_unavailable_records() {
        let mut api = probes()[0].clone();
        api["listenerState"] = json!("closed");
        api["serverState"] = json!("stopped");
        let mut worker = probes()[4].clone();
        worker["workerState"] = json!("not_running");
        worker["temporalNamespace"] = Value::Null;
        let mut runner = probes()[8].clone();
        runner["listenerState"] = json!("closed");
        runner["storageRecoveryComplete"] = json!(false);
        runner["volumeBinding"] = Value::Null;
        for value in [api, worker, runner] {
            let bytes = raw(&value);
            let parsed = parse_dependency_probe(&bytes).unwrap();
            assert_eq!(canonical_probe_bytes(&parsed).unwrap(), bytes);
        }
    }

    #[test]
    fn authority_rejects_numeric_aliases_ports_and_noncanonical_hosts() {
        for value in [
            "127.0.0.1:7233",
            "temporal.internal:7233",
            "gateway:443",
            "foo.0xyz:7233",
            "123.example:7233",
            "localhost:7233",
        ] {
            assert!(canonical_authority(value), "{value}");
        }
        for value in [
            "127.1:7233",
            "2130706433:7233",
            "0177.0.0.1:7233",
            "0x7f000001:7233",
            "0x7f.0.0.1:7233",
            "foo.0x:7233",
            "example.123:7233",
            "example.0x7f:7233",
            "0x:7233",
            "127.0.0.1.:7233",
            "Temporal:7233",
            "host.:7233",
            "-host:7233",
            "host-:7233",
            "a..b:7233",
            "host:0",
            "host:65536",
            "host:07233",
            "host:+7233",
            "host:",
            "host",
            "user@host:7233",
            "host:7233/path",
            "host:7233?secret",
            "[::1]:7233",
            "höst:7233",
            "host:7233\n",
            "host\\alias:7233",
        ] {
            assert!(!canonical_authority(value), "{value}");
        }
    }

    #[test]
    fn namespace_requires_bounded_nfc_control_free_text() {
        for text in [
            String::new(),
            "n".repeat(241),
            " name".into(),
            "name ".into(),
            "name\nvalue".into(),
            "e\u{301}".into(),
            "\u{fffd}".into(),
            "\u{feff}name".into(),
        ] {
            let mut value = probes()[3].clone();
            value["temporalNamespace"]["namespace"] = json!(text);
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
        for text in ["é".repeat(120), "n".repeat(240)] {
            let mut value = probes()[3].clone();
            value["temporalNamespace"]["namespace"] = json!(text);
            assert!(parse_dependency_probe(&raw(&value)).is_ok());
        }
        let mut value = probes()[3].clone();
        value["temporalNamespace"]["transport"] = json!("https");
        assert!(parse_dependency_probe(&raw(&value)).is_err());
        value["temporalNamespace"] = namespace();
        value["temporalNamespace"]["namespaceId"] = json!("n".repeat(129));
        assert!(parse_dependency_probe(&raw(&value)).is_err());
    }

    #[test]
    fn gateway_reference_requires_exact_field_grammars() {
        for (key, replacement) in [
            ("endpointId", json!("e".repeat(19))),
            ("runtimeInstanceId", json!("r".repeat(129))),
            ("transport", json!("tls")),
            ("observationSha256", json!("B".repeat(64))),
            ("transitionSha256", json!("a".repeat(63))),
            ("lifecycleState", json!("ready")),
        ] {
            let mut value = probes()[1].clone();
            value["gatewayControl"][key] = replacement;
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
        let mut value = probes()[1].clone();
        value["gatewayControl"]
            .as_object_mut()
            .unwrap()
            .remove("observationSha256");
        assert!(parse_dependency_probe(&raw(&value)).is_err());
    }

    #[test]
    fn source_verifier_reference_preserves_id_and_sequence_bounds() {
        for (key, replacement) in [
            ("bootId", json!("b".repeat(19))),
            ("startupRequestId", json!("s".repeat(129))),
            ("bootGeneration", json!(0)),
            ("parkedHeartbeatSequence", json!(-1)),
        ] {
            let mut value = probes()[7].clone();
            value["bootStartup"][key] = replacement;
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
        let mut value = probes()[7].clone();
        value["bootStartup"]["bootId"] = json!("boot:/unit-reference.648");
        assert!(parse_dependency_probe(&raw(&value)).is_ok());
    }

    #[test]
    fn runner_reference_requires_existing_canonical_storage_ids() {
        for (key, replacement) in [
            ("volumeId", json!(format!("{TOKEN}="))),
            ("processInstanceId", json!("r".repeat(20))),
            ("runtimeGrantId", json!("bad/grant")),
            ("enrollmentEpoch", json!(0)),
            ("storageAttestationGeneration", json!(0)),
            ("runtimeSha256", json!("G".repeat(64))),
        ] {
            let mut value = probes()[8].clone();
            value["volumeBinding"][key] = replacement;
            assert!(parse_dependency_probe(&raw(&value)).is_err());
        }
    }

    #[test]
    fn probe_reader_rejects_oversize_invalid_utf8_and_noncanonical_bytes() {
        assert!(parse_dependency_probe(&vec![b' '; MAX_PROBE_BYTES + 1]).is_err());
        assert!(parse_dependency_probe(&[0xff]).is_err());
        let bytes = raw(&probes()[0]);
        assert!(parse_dependency_probe(&bytes[..bytes.len() - 1]).is_err());
        let mut extra_lf = bytes;
        extra_lf.push(b'\n');
        assert!(parse_dependency_probe(&extra_lf).is_err());
    }

    #[test]
    fn canonical_probe_writer_revalidates_constructed_probe() {
        let mut probe = parse_dependency_probe(&raw(&probes()[5])).unwrap();
        if let DependencyProbe::DiscoveryWorker(control) = &mut probe {
            control.loop_state = LoopState::Stopped;
            control.control_progress = ControlProgress::Fresh;
        }
        assert!(canonical_probe_bytes(&probe).is_err());
    }

    #[test]
    fn parse_errors_do_not_echo_rejected_values() {
        let mut value = probes()[3].clone();
        value["temporalNamespace"]["authority"] = json!("private-input-do-not-log:0");
        let error = parse_dependency_probe(&raw(&value)).unwrap_err();
        assert_eq!(error.to_string(), "invalid managed cloud authority");
        assert!(!error.to_string().contains("private-input"));
    }
}
