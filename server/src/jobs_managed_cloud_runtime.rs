//! Process-local configuration for the signed managed-cloud release authority.
//!
//! Environment configuration selects one deployment scope; it never creates
//! release authority or readiness. Database callers must still resolve the
//! signed active head and fresh role quorum for this exact scope.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use reqwest::Url;
use tokio::{
    sync::watch,
    task::{AbortHandle, JoinHandle},
};

use crate::db::{
    jobs::{
        self, ClaimManagedCloudRuntimeGrant, ManagedCloudRegistryError,
        ManagedCloudRuntimeHeartbeat, ManagedCloudRuntimeHeartbeatInput,
        ManagedCloudRuntimeInstance, ManagedCloudRuntimeMeasurementIdentity, ManagedCloudScope,
    },
    DbPool,
};

const CLOUD_DISTRIBUTION_FLAG: &str = "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED";
const ENVIRONMENT: &str = "BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT";
const REGION: &str = "BLUEY_JOBS_MANAGED_CLOUD_REGION";
const CHANNEL: &str = "BLUEY_JOBS_MANAGED_CLOUD_CHANNEL";
const API_ORIGIN: &str = "BLUEY_JOBS_MANAGED_CLOUD_API_ORIGIN";
const RUNTIME_FLAG: &str = "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED";
const HEARTBEAT_SECONDS: &str = "BLUEY_JOBS_MANAGED_CLOUD_HEARTBEAT_SECONDS";
const WORKER_SIGNING_KEY: &str = "BLUEY_JOBS_WORKER_SIGNING_KEY";
const RUNTIME_COMPONENT_ID: &str = "jobs-api";
const RUNTIME_ARTIFACT_ROOT: &str = "/";
const RUNTIME_OPERATION_ATTEMPTS: usize = 3;
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerRuntimeRole {
    JobsApi,
    WorkflowCleanupDispatcher,
    WorkflowCommandDispatcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServerRuntimeEnvironment {
    grant_id: &'static str,
    grant_token: &'static str,
    runtime_instance_id: &'static str,
    worker_id: &'static str,
}

impl ServerRuntimeRole {
    const ALL: [Self; 3] = [
        Self::JobsApi,
        Self::WorkflowCleanupDispatcher,
        Self::WorkflowCommandDispatcher,
    ];

    fn authority(self) -> &'static str {
        match self {
            Self::JobsApi => "jobs_api",
            Self::WorkflowCleanupDispatcher => "workflow_cleanup_dispatcher",
            Self::WorkflowCommandDispatcher => "workflow_command_dispatcher",
        }
    }

    fn environment(self) -> ServerRuntimeEnvironment {
        match self {
            Self::JobsApi => ServerRuntimeEnvironment {
                grant_id: "BLUEY_JOBS_MANAGED_CLOUD_JOBS_API_GRANT_ID",
                grant_token: "BLUEY_JOBS_MANAGED_CLOUD_JOBS_API_GRANT_TOKEN",
                runtime_instance_id: "BLUEY_JOBS_MANAGED_CLOUD_JOBS_API_RUNTIME_INSTANCE_ID",
                worker_id: "BLUEY_JOBS_MANAGED_CLOUD_JOBS_API_WORKER_ID",
            },
            Self::WorkflowCleanupDispatcher => ServerRuntimeEnvironment {
                grant_id: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_CLEANUP_DISPATCHER_GRANT_ID",
                grant_token: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_CLEANUP_DISPATCHER_GRANT_TOKEN",
                runtime_instance_id:
                    "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_CLEANUP_DISPATCHER_RUNTIME_INSTANCE_ID",
                worker_id: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_CLEANUP_DISPATCHER_WORKER_ID",
            },
            Self::WorkflowCommandDispatcher => ServerRuntimeEnvironment {
                grant_id: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_COMMAND_DISPATCHER_GRANT_ID",
                grant_token: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_COMMAND_DISPATCHER_GRANT_TOKEN",
                runtime_instance_id:
                    "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_COMMAND_DISPATCHER_RUNTIME_INSTANCE_ID",
                worker_id: "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_COMMAND_DISPATCHER_WORKER_ID",
            },
        }
    }
}

struct ServerRuntimeCredentials {
    grant_id: String,
    grant_token: String,
    runtime_instance_id: String,
    worker_id: String,
    session_token: String,
}

impl ServerRuntimeCredentials {
    fn from_environment(role: ServerRuntimeRole) -> anyhow::Result<Self> {
        let environment = role.environment();
        let grant_id = required_role_environment(environment.grant_id)?;
        let grant_token = required_role_environment(environment.grant_token)?;
        let runtime_instance_id = required_role_environment(environment.runtime_instance_id)?;
        let worker_id = required_role_environment(environment.worker_id)?;
        anyhow::ensure!(
            valid_runtime_route_id(&grant_id)
                && valid_runtime_route_id(&runtime_instance_id)
                && valid_runtime_route_id(&worker_id),
            "managed-cloud {} runtime identity configuration is invalid",
            role.authority()
        );
        let session_token = jobs::derive_managed_cloud_runtime_session_token(
            &grant_token,
            &grant_id,
            &runtime_instance_id,
        )
        .with_context(|| {
            format!(
                "derive managed-cloud {} runtime session token",
                role.authority()
            )
        })?;
        Ok(Self {
            grant_id,
            grant_token,
            runtime_instance_id,
            worker_id,
            session_token,
        })
    }
}

struct ServerRuntimeConfig {
    role: ServerRuntimeRole,
    scope: ManagedCloudScope,
    heartbeat_interval: Duration,
    artifact_root: PathBuf,
    measurement: ManagedCloudRuntimeMeasurementIdentity,
    credentials: ServerRuntimeCredentials,
}

impl ServerRuntimeConfig {
    fn from_environment(
        role: ServerRuntimeRole,
        artifact_root: &Path,
        scope: &ManagedCloudScope,
        heartbeat_interval: Duration,
    ) -> anyhow::Result<Self> {
        let measurement = jobs::inspect_managed_cloud_runtime_identity(
            artifact_root,
            RUNTIME_COMPONENT_ID,
            role.authority(),
        )
        .with_context(|| {
            format!(
                "inspect managed-cloud {} runtime identity",
                role.authority()
            )
        })?;
        Ok(Self {
            role,
            scope: scope.clone(),
            heartbeat_interval,
            artifact_root: artifact_root.to_path_buf(),
            measurement,
            credentials: ServerRuntimeCredentials::from_environment(role)?,
        })
    }

    fn revalidate_measurement(&self) -> anyhow::Result<()> {
        let current = jobs::inspect_managed_cloud_runtime_identity(
            &self.artifact_root,
            RUNTIME_COMPONENT_ID,
            self.role.authority(),
        )
        .with_context(|| {
            format!(
                "reinspect managed-cloud {} runtime identity",
                self.role.authority()
            )
        })?;
        anyhow::ensure!(
            current == self.measurement,
            "managed-cloud {} runtime measurement changed during startup",
            self.role.authority()
        );
        Ok(())
    }
}

#[derive(Clone)]
enum ReporterLiveness {
    JobsApi,
    WorkflowCleanupDispatcher(AbortHandle),
    WorkflowCommandDispatcher(AbortHandle),
}

impl ReporterLiveness {
    fn health(&self) -> ReporterHealth {
        if !managed_cloud_runtime_enabled() {
            return ReporterHealth::Draining;
        }
        match self {
            Self::JobsApi => ReporterHealth::Ready,
            Self::WorkflowCleanupDispatcher(handle) => dispatcher_reporter_health(
                crate::jobs_workflow_cleanup::workflow_cleanup_dispatch_configured_for_runtime(),
                handle.is_finished(),
            ),
            Self::WorkflowCommandDispatcher(handle) => dispatcher_reporter_health(
                crate::jobs_workflow_dispatch::workflow_command_dispatch_configured_for_admission(),
                handle.is_finished(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReporterHealth {
    Ready,
    Draining,
    DependencyUnavailable,
}

impl ReporterHealth {
    fn state(self) -> (&'static str, Option<String>) {
        match self {
            Self::Ready => ("ready", None),
            Self::Draining => ("draining", Some("draining".to_string())),
            Self::DependencyUnavailable => ("degraded", Some("dependency_unavailable".to_string())),
        }
    }
}

fn dispatcher_reporter_health(configured: bool, handle_finished: bool) -> ReporterHealth {
    if configured && !handle_finished {
        ReporterHealth::Ready
    } else {
        ReporterHealth::DependencyUnavailable
    }
}

struct ActiveRuntimeReporter {
    role: ServerRuntimeRole,
    heartbeat_interval: Duration,
    session_token: String,
    instance: ManagedCloudRuntimeInstance,
    next_heartbeat_sequence: i64,
    liveness: ReporterLiveness,
}

struct RuntimeReporterTask {
    role: ServerRuntimeRole,
    handle: JoinHandle<()>,
}

/// Owns the three role-separated reporters embedded in the Jobs API image.
///
/// Callers start this only after the API listener is bound and drain it as soon
/// as the HTTP serving future ends. Dispatcher evidence is independently tied
/// to each actual dispatcher task, so one live task cannot satisfy another
/// role's quorum.
pub struct JobsApiManagedCloudRuntimeReporters {
    shutdown: watch::Sender<bool>,
    tasks: Vec<RuntimeReporterTask>,
}

impl JobsApiManagedCloudRuntimeReporters {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let mut tasks = self.tasks;
        let drained = tokio::time::timeout(RUNTIME_SHUTDOWN_TIMEOUT, async {
            for task in &mut tasks {
                match (&mut task.handle).await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() => {}
                    Err(error) => tracing::warn!(
                        role = task.role.authority(),
                        error = %error,
                        "managed-cloud runtime reporter stopped unexpectedly"
                    ),
                }
            }
        })
        .await;
        if drained.is_err() {
            tracing::warn!("managed-cloud runtime reporter drain timed out");
            for task in tasks {
                if !task.handle.is_finished() {
                    task.handle.abort();
                }
                match task.handle.await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() => {}
                    Err(error) => tracing::warn!(
                        role = task.role.authority(),
                        error = %error,
                        "managed-cloud runtime reporter stopped unexpectedly after drain timeout"
                    ),
                }
            }
        }
    }
}

pub fn validate_runtime_config() -> anyhow::Result<()> {
    let cloud_distribution = release_flag_enabled(CLOUD_DISTRIBUTION_FLAG);
    let command_dispatch = crate::jobs_workflow_dispatch::workflow_command_dispatch_enabled();
    let runtime_evidence = exact_true_flag_enabled(RUNTIME_FLAG);
    if !cloud_distribution && !command_dispatch && !runtime_evidence {
        return Ok(());
    }
    anyhow::ensure!(
        crate::jobs_workflow_dispatch::workflow_command_dispatch_configured_for_admission(),
        "managed cloud launch requires valid workflow-command dispatcher configuration"
    );
    let scope = managed_cloud_scope_from_environment()?;
    managed_cloud_api_origin()?;
    anyhow::ensure!(
        !(cfg!(debug_assertions) && scope.environment == "production"),
        "a debug server cannot select the production managed-cloud scope"
    );
    if runtime_evidence {
        anyhow::ensure!(
            crate::jobs_workflow_cleanup::workflow_cleanup_dispatch_configured_for_runtime(),
            "managed cloud launch requires valid workflow-cleanup dispatcher configuration"
        );
        validate_worker_signing_key()?;
        load_server_runtime_configs(Path::new(RUNTIME_ARTIFACT_ROOT), &scope)?;
    }
    Ok(())
}

/// Claims and starts the three role-separated reporters in the Jobs API image.
///
/// The listener reference proves that API readiness is not emitted before the
/// socket is bound. All configuration, artifact measurements, dispatcher task
/// handles, and role identities are checked before the first durable claim.
pub fn start_jobs_api_managed_cloud_runtime_reporters(
    pool: DbPool,
    listener: &tokio::net::TcpListener,
    workflow_command_dispatcher: Option<&JoinHandle<()>>,
    workflow_cleanup_dispatcher: Option<&JoinHandle<()>>,
) -> anyhow::Result<Option<JobsApiManagedCloudRuntimeReporters>> {
    if !managed_cloud_runtime_enabled() {
        return Ok(None);
    }
    tokio::runtime::Handle::try_current()
        .context("managed-cloud runtime reporters require a Tokio runtime")?;
    validate_runtime_config()?;
    listener
        .local_addr()
        .context("resolve bound Jobs API listener for managed-cloud readiness")?;
    let command_handle = workflow_command_dispatcher
        .context("managed-cloud command-dispatcher reporter requires a live dispatcher task")?;
    let cleanup_handle = workflow_cleanup_dispatcher
        .context("managed-cloud cleanup-dispatcher reporter requires a live dispatcher task")?;
    anyhow::ensure!(
        !command_handle.is_finished() && !cleanup_handle.is_finished(),
        "managed-cloud dispatcher task stopped before runtime claim"
    );
    let scope = managed_cloud_scope_from_environment()?;
    let configs = load_server_runtime_configs(Path::new(RUNTIME_ARTIFACT_ROOT), &scope)?;
    let command_abort = command_handle.abort_handle();
    let cleanup_abort = cleanup_handle.abort_handle();
    let liveness = [
        ReporterLiveness::JobsApi,
        ReporterLiveness::WorkflowCleanupDispatcher(cleanup_abort),
        ReporterLiveness::WorkflowCommandDispatcher(command_abort),
    ];
    for dependency in &liveness {
        anyhow::ensure!(
            dependency.health() == ReporterHealth::Ready,
            "managed-cloud runtime dependency is not ready before claim"
        );
    }

    let mut reporters = Vec::with_capacity(ServerRuntimeRole::ALL.len());
    for (config, dependency) in configs.into_iter().zip(liveness) {
        reporters.push(claim_and_start_reporter(&pool, config, dependency)?);
    }
    let (shutdown, receiver) = watch::channel(false);
    let tasks = reporters
        .into_iter()
        .map(|reporter| {
            let role = reporter.role;
            let pool = pool.clone();
            let shutdown = receiver.clone();
            RuntimeReporterTask {
                role,
                handle: tokio::spawn(run_runtime_reporter(pool, reporter, shutdown)),
            }
        })
        .collect();
    tracing::info!(
        roles = ServerRuntimeRole::ALL.len(),
        "managed-cloud Jobs API runtime reporters started"
    );
    Ok(Some(JobsApiManagedCloudRuntimeReporters {
        shutdown,
        tasks,
    }))
}

fn load_server_runtime_configs(
    artifact_root: &Path,
    scope: &ManagedCloudScope,
) -> anyhow::Result<Vec<ServerRuntimeConfig>> {
    reject_configured_runtime_identity()?;
    validate_running_jobs_api_executable(artifact_root)?;
    let heartbeat_interval = managed_cloud_heartbeat_interval()?;
    let configs: Vec<_> = ServerRuntimeRole::ALL
        .into_iter()
        .map(|role| {
            ServerRuntimeConfig::from_environment(role, artifact_root, scope, heartbeat_interval)
        })
        .collect::<anyhow::Result<_>>()?;
    for (index, config) in configs.iter().enumerate() {
        for peer in configs.iter().skip(index + 1) {
            anyhow::ensure!(
                config.credentials.grant_id != peer.credentials.grant_id
                    && config.credentials.grant_token != peer.credentials.grant_token
                    && config.credentials.runtime_instance_id
                        != peer.credentials.runtime_instance_id
                    && config.credentials.worker_id != peer.credentials.worker_id
                    && config.credentials.session_token != peer.credentials.session_token,
                "managed-cloud server runtime credentials must be role-separated"
            );
        }
    }
    Ok(configs)
}

fn validate_running_jobs_api_executable(artifact_root: &Path) -> anyhow::Result<()> {
    let expected = artifact_root.join("usr/local/bin/bluey-jobs-api");
    let metadata = std::fs::symlink_metadata(&expected)
        .context("inspect managed-cloud Jobs API executable")?;
    anyhow::ensure!(
        metadata.file_type().is_file() && metadata.len() > 0,
        "managed-cloud Jobs API executable is not a nonempty regular file"
    );
    let expected = std::fs::canonicalize(expected)
        .context("canonicalize measured managed-cloud Jobs API executable")?;
    let current =
        std::env::current_exe().context("resolve running managed-cloud Jobs API executable")?;
    let current = std::fs::canonicalize(current)
        .context("canonicalize running managed-cloud Jobs API executable")?;
    anyhow::ensure!(
        current == expected,
        "managed-cloud runtime roles require the measured Bluey Jobs API executable"
    );
    Ok(())
}

fn reject_configured_runtime_identity() -> anyhow::Result<()> {
    let prefix = ["BLUEY_", "JOBS_MANAGED_", "CLOUD_"].concat();
    let identity = ["RUNTIME_", "IDENTITY_", "SHA256"].concat();
    let unscoped = format!("{prefix}{identity}");
    let role_suffix = format!("_{identity}");
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        let role_scoped = name.starts_with(prefix.as_str())
            && name.ends_with(role_suffix.as_str())
            && name.len() > prefix.len() + role_suffix.len();
        anyhow::ensure!(
            name.as_ref() != unscoped.as_str() && !role_scoped,
            "managed-cloud runtime identity cannot be configured"
        );
    }
    Ok(())
}

fn claim_and_start_reporter(
    pool: &DbPool,
    config: ServerRuntimeConfig,
    liveness: ReporterLiveness,
) -> anyhow::Result<ActiveRuntimeReporter> {
    let claim = ClaimManagedCloudRuntimeGrant {
        grant_id: config.credentials.grant_id.clone(),
        grant_token: config.credentials.grant_token.clone(),
        runtime_instance_id: config.credentials.runtime_instance_id.clone(),
        worker_id: config.credentials.worker_id.clone(),
        session_token: config.credentials.session_token.clone(),
        runtime_identity_sha256: config.measurement.runtime_identity_sha256.clone(),
    };
    let instance = retry_exact_managed_cloud_operation(|| {
        jobs::claim_managed_cloud_runtime_grant(pool, &claim)
    })
    .with_context(|| {
        format!(
            "claim managed-cloud {} runtime grant",
            config.role.authority()
        )
    })?;
    validate_claimed_instance(&config, &instance)?;
    config.revalidate_measurement()?;
    let health = liveness.health();
    let input = heartbeat_input(
        &instance,
        &config.credentials.session_token,
        instance.next_heartbeat_sequence,
        health,
    );
    let heartbeat = retry_exact_managed_cloud_operation(|| {
        jobs::record_managed_cloud_runtime_heartbeat(pool, &input)
    })
    .with_context(|| {
        format!(
            "record initial managed-cloud {} runtime heartbeat",
            config.role.authority()
        )
    })?;
    validate_heartbeat_response(&instance, &input, &heartbeat)?;
    anyhow::ensure!(
        heartbeat.health_state == "ready",
        "managed-cloud {} runtime dependency stopped during startup",
        config.role.authority()
    );
    Ok(ActiveRuntimeReporter {
        role: config.role,
        heartbeat_interval: config.heartbeat_interval,
        session_token: config.credentials.session_token,
        next_heartbeat_sequence: heartbeat.heartbeat_sequence + 1,
        instance,
        liveness,
    })
}

async fn run_runtime_reporter(
    pool: DbPool,
    mut reporter: ActiveRuntimeReporter,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let forced_health = tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    Some(ReporterHealth::Draining)
                } else {
                    None
                }
            }
            _ = tokio::time::sleep(reporter.heartbeat_interval) => None,
        };
        let health = forced_health.unwrap_or_else(|| reporter.liveness.health());
        let input = heartbeat_input(
            &reporter.instance,
            &reporter.session_token,
            reporter.next_heartbeat_sequence,
            health,
        );
        let heartbeat = retry_exact_managed_cloud_operation(|| {
            jobs::record_managed_cloud_runtime_heartbeat(&pool, &input)
        });
        let heartbeat = match heartbeat {
            Ok(heartbeat) => heartbeat,
            Err(error) => {
                tracing::warn!(
                    role = reporter.role.authority(),
                    reason_code = "managed_cloud_runtime_heartbeat_failed",
                    error = %error,
                    "managed-cloud runtime reporter stopped"
                );
                return;
            }
        };
        if let Err(error) = validate_heartbeat_response(&reporter.instance, &input, &heartbeat) {
            tracing::warn!(
                role = reporter.role.authority(),
                reason_code = "managed_cloud_runtime_heartbeat_identity_conflict",
                error = %error,
                "managed-cloud runtime reporter stopped"
            );
            return;
        }
        reporter.next_heartbeat_sequence = heartbeat.heartbeat_sequence + 1;
        if health != ReporterHealth::Ready {
            return;
        }
    }
}

fn heartbeat_input(
    instance: &ManagedCloudRuntimeInstance,
    session_token: &str,
    heartbeat_sequence: i64,
    health: ReporterHealth,
) -> ManagedCloudRuntimeHeartbeatInput {
    let (health_state, reason_code) = health.state();
    ManagedCloudRuntimeHeartbeatInput {
        runtime_instance_id: instance.runtime_instance_id.clone(),
        worker_id: instance.worker_id.clone(),
        session_token: session_token.to_string(),
        heartbeat_sequence,
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
        health_state: health_state.to_string(),
        reason_code,
    }
}

fn validate_claimed_instance(
    config: &ServerRuntimeConfig,
    instance: &ManagedCloudRuntimeInstance,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        instance.grant_id == config.credentials.grant_id
            && instance.runtime_instance_id == config.credentials.runtime_instance_id
            && instance.worker_id == config.credentials.worker_id
            && instance.runtime_identity_sha256 == config.measurement.runtime_identity_sha256
            && instance.scope == config.scope
            && instance.component_id == RUNTIME_COMPONENT_ID
            && instance.role == config.role.authority()
            && instance.config_schema_sha256 == config.measurement.config_schema_sha256
            && instance.migration_set_sha256 == config.measurement.migration_set_sha256
            && instance.protocol_set_sha256 == config.measurement.protocol_set_sha256
            && instance.head_revision > 0
            && instance.instance_epoch > 0
            && instance.next_heartbeat_sequence > 0
            && instance.claimed_at_ms > 0
            && instance.activation_expires_at_ms > instance.claimed_at_ms,
        "managed-cloud {} runtime claim identity is inconsistent",
        config.role.authority()
    );
    let dependency_evidence = jobs::managed_cloud_dependency_evidence_sha256(
        &instance.role,
        &instance.activation_sha256,
        &instance.manifest_sha256,
        &instance.component_id,
        &instance.artifact_sha256,
        &instance.task_queue_sha256,
        &instance.failure_converter_sha256,
    )
    .context("derive claimed managed-cloud dependency evidence")?;
    anyhow::ensure!(
        dependency_evidence == instance.dependency_evidence_sha256,
        "managed-cloud {} runtime dependency evidence is inconsistent",
        config.role.authority()
    );
    Ok(())
}

fn validate_heartbeat_response(
    instance: &ManagedCloudRuntimeInstance,
    input: &ManagedCloudRuntimeHeartbeatInput,
    heartbeat: &ManagedCloudRuntimeHeartbeat,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        heartbeat.runtime_instance_id == input.runtime_instance_id
            && heartbeat.worker_id == input.worker_id
            && heartbeat.instance_epoch == instance.instance_epoch
            && heartbeat.heartbeat_sequence == input.heartbeat_sequence
            && heartbeat.observed_head_revision == input.observed_head_revision
            && heartbeat.observed_transition_sha256 == input.observed_transition_sha256
            && heartbeat.activation_sha256 == input.activation_sha256
            && heartbeat.manifest_sha256 == input.manifest_sha256
            && heartbeat.component_id == input.component_id
            && heartbeat.role == input.role
            && heartbeat.artifact_sha256 == input.artifact_sha256
            && heartbeat.migration_set_sha256 == input.migration_set_sha256
            && heartbeat.config_schema_sha256 == input.config_schema_sha256
            && heartbeat.protocol_set_sha256 == input.protocol_set_sha256
            && heartbeat.task_queue_sha256 == input.task_queue_sha256
            && heartbeat.failure_converter_sha256 == input.failure_converter_sha256
            && heartbeat.dependency_evidence_sha256 == input.dependency_evidence_sha256
            && heartbeat.health_state == input.health_state
            && heartbeat.reason_code == input.reason_code
            && heartbeat.heartbeat_at_ms > 0,
        "managed-cloud runtime heartbeat response identity is inconsistent"
    );
    Ok(())
}

fn retry_exact_managed_cloud_operation<T>(
    mut operation: impl FnMut() -> Result<T, ManagedCloudRegistryError>,
) -> Result<T, ManagedCloudRegistryError> {
    let mut last_error = match operation() {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    for _ in 1..RUNTIME_OPERATION_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn managed_cloud_runtime_enabled() -> bool {
    exact_true_flag_enabled(RUNTIME_FLAG)
}

fn required_role_environment(name: &str) -> anyhow::Result<String> {
    let value = std::env::var(name).with_context(|| format!("{name} is required"))?;
    anyhow::ensure!(
        value == value.trim() && !value.is_empty(),
        "{name} is invalid"
    );
    Ok(value)
}

fn valid_runtime_route_id(value: &str) -> bool {
    (20..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn managed_cloud_heartbeat_interval() -> anyhow::Result<Duration> {
    let raw = std::env::var(HEARTBEAT_SECONDS)
        .with_context(|| format!("{HEARTBEAT_SECONDS} is required"))?;
    anyhow::ensure!(raw == raw.trim(), "{HEARTBEAT_SECONDS} is invalid");
    let seconds = raw
        .parse::<u64>()
        .with_context(|| format!("{HEARTBEAT_SECONDS} must be an integer"))?;
    anyhow::ensure!(
        (1..=60).contains(&seconds),
        "{HEARTBEAT_SECONDS} must be between 1 and 60"
    );
    Ok(Duration::from_secs(seconds))
}

fn validate_worker_signing_key() -> anyhow::Result<()> {
    let key = std::env::var(WORKER_SIGNING_KEY)
        .with_context(|| format!("{WORKER_SIGNING_KEY} is required"))?;
    anyhow::ensure!(
        (32..=4_096).contains(&key.len()),
        "{WORKER_SIGNING_KEY} is invalid"
    );
    Ok(())
}

pub(crate) fn managed_cloud_api_origin_for_worker_auth() -> Option<String> {
    managed_cloud_api_origin().ok()
}

pub(crate) fn managed_cloud_scope_for_admission() -> Option<ManagedCloudScope> {
    let scope = managed_cloud_scope_for_runtime()?;
    (scope.channel != "shadow").then_some(scope)
}

pub(crate) fn managed_cloud_scope_for_runtime() -> Option<ManagedCloudScope> {
    let scope = managed_cloud_scope_from_environment().ok()?;
    (!(cfg!(debug_assertions) && scope.environment == "production")).then_some(scope)
}

fn managed_cloud_scope_from_environment() -> anyhow::Result<ManagedCloudScope> {
    managed_cloud_scope(
        &std::env::var(ENVIRONMENT).with_context(|| format!("{ENVIRONMENT} is required"))?,
        &std::env::var(REGION).with_context(|| format!("{REGION} is required"))?,
        &std::env::var(CHANNEL).with_context(|| format!("{CHANNEL} is required"))?,
    )
}

fn managed_cloud_api_origin() -> anyhow::Result<String> {
    let raw = std::env::var(API_ORIGIN).with_context(|| format!("{API_ORIGIN} is required"))?;
    anyhow::ensure!(
        raw == raw.trim() && !raw.is_empty(),
        "managed-cloud API origin is invalid"
    );
    let mut origin = Url::parse(&raw).context("parse managed-cloud API origin")?;
    let loopback_http = cfg!(debug_assertions)
        && origin.scheme() == "http"
        && origin
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    anyhow::ensure!(
        origin.scheme() == "https" || loopback_http,
        "managed-cloud API origin must use HTTPS"
    );
    anyhow::ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.query().is_none()
            && origin.fragment().is_none()
            && matches!(origin.path(), "" | "/"),
        "managed-cloud API origin must not contain credentials, path, query, or fragment"
    );
    origin.set_path("");
    Ok(origin.as_str().trim_end_matches('/').to_string())
}

fn managed_cloud_scope(
    environment: &str,
    region: &str,
    channel: &str,
) -> anyhow::Result<ManagedCloudScope> {
    anyhow::ensure!(
        matches!(environment, "staging" | "production"),
        "managed-cloud environment is invalid"
    );
    anyhow::ensure!(valid_region(region), "managed-cloud region is invalid");
    anyhow::ensure!(
        matches!(channel, "shadow" | "canary" | "general"),
        "managed-cloud channel is invalid"
    );
    Ok(ManagedCloudScope {
        environment: environment.to_string(),
        region: region.to_string(),
        channel: channel.to_string(),
    })
}

fn valid_region(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 || !value.is_ascii() {
        return false;
    }
    let bytes = value.as_bytes();
    let first_is_alphanumeric = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    let last = bytes[bytes.len() - 1];
    let last_is_alphanumeric = last.is_ascii_lowercase() || last.is_ascii_digit();
    first_is_alphanumeric
        && last_is_alphanumeric
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn release_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn exact_true_flag_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "true")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    struct EnvironmentGuard {
        prior: Vec<(String, Option<OsString>)>,
    }

    impl EnvironmentGuard {
        fn new() -> Self {
            Self { prior: Vec::new() }
        }

        fn set(&mut self, name: impl Into<String>, value: impl AsRef<str>) {
            let name = name.into();
            if !self.prior.iter().any(|(candidate, _)| candidate == &name) {
                self.prior.push((name.clone(), std::env::var_os(&name)));
            }
            std::env::set_var(name, value.as_ref());
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            for (name, value) in self.prior.drain(..).rev() {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    #[test]
    fn managed_cloud_scope_is_closed_and_has_no_default() {
        let exact = managed_cloud_scope("staging", "us-east-1", "shadow").unwrap();
        assert_eq!(exact.environment, "staging");
        assert_eq!(exact.region, "us-east-1");
        assert_eq!(exact.channel, "shadow");

        for (environment, region, channel) in [
            ("", "us-east-1", "shadow"),
            ("development", "us-east-1", "shadow"),
            ("staging", "Us-east-1", "shadow"),
            ("staging", "-us-east-1", "shadow"),
            ("staging", "us-east-1-", "shadow"),
            ("staging", "us_east_1", "shadow"),
            ("staging", "us-east-1", "beta"),
        ] {
            assert!(managed_cloud_scope(environment, region, channel).is_err());
        }
    }

    #[test]
    #[serial_test::serial]
    fn shadow_scope_can_report_runtime_evidence_but_never_admits_customers() {
        let prior = [ENVIRONMENT, REGION, CHANNEL].map(|name| (name, std::env::var_os(name)));
        std::env::set_var(ENVIRONMENT, "staging");
        std::env::set_var(REGION, "us-east-1");
        std::env::set_var(CHANNEL, "shadow");
        assert_eq!(
            managed_cloud_scope_for_runtime()
                .expect("shadow runtime scope")
                .channel,
            "shadow"
        );
        assert!(managed_cloud_scope_for_admission().is_none());
        for (name, value) in prior {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    #[test]
    fn region_contract_accepts_only_bounded_lowercase_dns_labels() {
        let longest = "a".repeat(64);
        for valid in ["a", "1", "us-east-1", longest.as_str()] {
            assert!(valid_region(valid), "{valid}");
        }
        let too_long = "a".repeat(65);
        for invalid in ["", "-a", "a-", "A", "a_b", too_long.as_str()] {
            assert!(!valid_region(invalid), "{invalid}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn worker_auth_origin_is_exact_and_normalized() {
        let prior = std::env::var_os(API_ORIGIN);
        std::env::set_var(API_ORIGIN, "https://JOBS-API.internal:443/");
        assert_eq!(
            managed_cloud_api_origin().unwrap(),
            "https://jobs-api.internal"
        );
        for invalid in [
            "https://jobs-api.internal/path",
            "https://user@jobs-api.internal",
            "https://jobs-api.internal?query=1",
            " https://jobs-api.internal",
        ] {
            std::env::set_var(API_ORIGIN, invalid);
            assert!(managed_cloud_api_origin().is_err(), "{invalid}");
        }
        match prior {
            Some(value) => std::env::set_var(API_ORIGIN, value),
            None => std::env::remove_var(API_ORIGIN),
        }
    }

    #[test]
    #[serial_test::serial]
    fn runtime_reporters_require_the_exact_true_flag() {
        let mut environment = EnvironmentGuard::new();
        for (value, expected) in [
            ("true", true),
            ("1", false),
            ("yes", false),
            ("TRUE", false),
            ("true ", false),
            ("false", false),
        ] {
            environment.set(RUNTIME_FLAG, value);
            assert_eq!(managed_cloud_runtime_enabled(), expected, "{value}");
        }
    }

    #[test]
    #[serial_test::serial]
    fn runtime_credentials_are_role_separated_and_derive_exact_sessions() {
        let mut environment = EnvironmentGuard::new();
        let mut sessions = Vec::new();
        for (index, role) in ServerRuntimeRole::ALL.into_iter().enumerate() {
            let grant_id = format!("grant-{}-1234567890", role.authority());
            let runtime_instance_id = format!("instance-{}-1234567890", role.authority());
            let worker_id = format!("worker-{}-1234567890", role.authority());
            let grant_token = format!("{}{}", "A".repeat(42), ["A", "Q", "g"][index]);
            let names = role.environment();
            environment.set(names.grant_id, &grant_id);
            environment.set(names.grant_token, &grant_token);
            environment.set(names.runtime_instance_id, &runtime_instance_id);
            environment.set(names.worker_id, &worker_id);

            let credentials = ServerRuntimeCredentials::from_environment(role).unwrap();
            assert_eq!(credentials.grant_id, grant_id);
            assert_eq!(credentials.runtime_instance_id, runtime_instance_id);
            assert_eq!(credentials.worker_id, worker_id);
            assert_eq!(
                credentials.session_token,
                jobs::derive_managed_cloud_runtime_session_token(
                    &grant_token,
                    &grant_id,
                    &runtime_instance_id,
                )
                .unwrap()
            );
            sessions.push(credentials.session_token);
        }
        sessions.sort();
        sessions.dedup();
        assert_eq!(sessions.len(), ServerRuntimeRole::ALL.len());
        assert_eq!(
            ServerRuntimeRole::JobsApi.environment().grant_id,
            "BLUEY_JOBS_MANAGED_CLOUD_JOBS_API_GRANT_ID"
        );
        assert_eq!(
            ServerRuntimeRole::WorkflowCleanupDispatcher
                .environment()
                .worker_id,
            "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_CLEANUP_DISPATCHER_WORKER_ID"
        );
        assert_eq!(
            ServerRuntimeRole::WorkflowCommandDispatcher
                .environment()
                .grant_token,
            "BLUEY_JOBS_MANAGED_CLOUD_WORKFLOW_COMMAND_DISPATCHER_GRANT_TOKEN"
        );
    }

    #[test]
    fn dispatcher_health_requires_configuration_and_a_live_task() {
        assert_eq!(
            dispatcher_reporter_health(true, false),
            ReporterHealth::Ready
        );
        assert_eq!(
            dispatcher_reporter_health(false, false),
            ReporterHealth::DependencyUnavailable
        );
        assert_eq!(
            dispatcher_reporter_health(true, true),
            ReporterHealth::DependencyUnavailable
        );
    }

    #[tokio::test]
    async fn dispatcher_health_drops_after_the_actual_task_exits_cleanly() {
        let task = tokio::spawn(async {});
        let task_health = task.abort_handle();
        task.await.unwrap();
        assert!(task_health.is_finished());
        assert_eq!(
            dispatcher_reporter_health(true, task_health.is_finished()),
            ReporterHealth::DependencyUnavailable
        );
    }

    #[test]
    fn heartbeat_input_reuses_the_claimed_release_and_instance_fences() {
        let instance = runtime_instance_fixture();
        let session_token = "A".repeat(43);
        let ready = heartbeat_input(&instance, &session_token, 9, ReporterHealth::Ready);
        assert_eq!(ready.runtime_instance_id, instance.runtime_instance_id);
        assert_eq!(ready.worker_id, instance.worker_id);
        assert_eq!(ready.heartbeat_sequence, 9);
        assert_eq!(ready.observed_head_revision, instance.head_revision);
        assert_eq!(ready.observed_transition_sha256, instance.transition_sha256);
        assert_eq!(ready.activation_sha256, instance.activation_sha256);
        assert_eq!(ready.manifest_sha256, instance.manifest_sha256);
        assert_eq!(ready.component_id, RUNTIME_COMPONENT_ID);
        assert_eq!(ready.role, "jobs_api");
        assert_eq!(ready.health_state, "ready");
        assert_eq!(ready.reason_code, None);

        let draining = heartbeat_input(&instance, &session_token, 10, ReporterHealth::Draining);
        assert_eq!(draining.health_state, "draining");
        assert_eq!(draining.reason_code.as_deref(), Some("draining"));
    }

    #[test]
    fn response_loss_retries_the_same_operation_before_returning() {
        let frozen_request = "runtime-instance-1234567890";
        let mut observed = Vec::new();
        let result = retry_exact_managed_cloud_operation(|| {
            observed.push(frozen_request);
            if observed.len() == 1 {
                Err(ManagedCloudRegistryError::Unavailable)
            } else {
                Ok(17)
            }
        })
        .unwrap();
        assert_eq!(result, 17);
        assert_eq!(observed, vec![frozen_request, frozen_request]);
    }

    #[test]
    fn claimed_runtime_requires_each_locally_measured_contract_digest() {
        let instance = runtime_instance_fixture();
        let config = server_runtime_config_fixture(&instance);
        validate_claimed_instance(&config, &instance).unwrap();

        let mut swapped = instance.clone();
        swapped.config_schema_sha256 = instance.migration_set_sha256.clone();
        assert!(validate_claimed_instance(&config, &swapped).is_err());

        let mut swapped = instance.clone();
        swapped.migration_set_sha256 = instance.protocol_set_sha256.clone();
        assert!(validate_claimed_instance(&config, &swapped).is_err());

        let mut swapped = instance.clone();
        swapped.protocol_set_sha256 = instance.config_schema_sha256.clone();
        assert!(validate_claimed_instance(&config, &swapped).is_err());
    }

    #[test]
    #[serial_test::serial]
    fn configured_runtime_identity_is_rejected_without_a_literal_config_name() {
        let mut environment = EnvironmentGuard::new();
        let prefix = ["BLUEY_", "JOBS_MANAGED_", "CLOUD_"].concat();
        let identity = ["RUNTIME_", "IDENTITY_", "SHA256"].concat();
        environment.set(format!("{prefix}JOBS_API_{identity}"), "0".repeat(64));
        assert!(reject_configured_runtime_identity().is_err());
    }

    fn runtime_instance_fixture() -> ManagedCloudRuntimeInstance {
        let mut instance = ManagedCloudRuntimeInstance {
            grant_id: "grant-jobs-api-1234567890".to_string(),
            runtime_instance_id: "instance-jobs-api-1234567890".to_string(),
            runtime_identity_sha256: "0".repeat(64),
            worker_id: "worker-jobs-api-1234567890".to_string(),
            scope: ManagedCloudScope {
                environment: "staging".to_string(),
                region: "us-east-1".to_string(),
                channel: "shadow".to_string(),
            },
            activation_sha256: "1".repeat(64),
            manifest_sha256: "2".repeat(64),
            component_id: RUNTIME_COMPONENT_ID.to_string(),
            role: "jobs_api".to_string(),
            head_revision: 3,
            transition_sha256: "4".repeat(64),
            artifact_sha256: "5".repeat(64),
            config_schema_sha256: "6".repeat(64),
            migration_set_sha256: "7".repeat(64),
            protocol_set_sha256: "8".repeat(64),
            task_queue_sha256: "9".repeat(64),
            failure_converter_sha256: "a".repeat(64),
            dependency_evidence_sha256: "b".repeat(64),
            activation_expires_at_ms: 1_800_000_000_000,
            instance_epoch: 1,
            next_heartbeat_sequence: 9,
            claimed_at_ms: 1_700_000_000_000,
            replayed: false,
        };
        instance.dependency_evidence_sha256 = jobs::managed_cloud_dependency_evidence_sha256(
            &instance.role,
            &instance.activation_sha256,
            &instance.manifest_sha256,
            &instance.component_id,
            &instance.artifact_sha256,
            &instance.task_queue_sha256,
            &instance.failure_converter_sha256,
        )
        .unwrap();
        instance
    }

    fn server_runtime_config_fixture(
        instance: &ManagedCloudRuntimeInstance,
    ) -> ServerRuntimeConfig {
        ServerRuntimeConfig {
            role: ServerRuntimeRole::JobsApi,
            scope: instance.scope.clone(),
            heartbeat_interval: Duration::from_secs(5),
            artifact_root: PathBuf::from("/"),
            measurement: ManagedCloudRuntimeMeasurementIdentity {
                runtime_measurement_sha256: "c".repeat(64),
                runtime_identity_sha256: instance.runtime_identity_sha256.clone(),
                config_schema_sha256: instance.config_schema_sha256.clone(),
                migration_set_sha256: instance.migration_set_sha256.clone(),
                protocol_set_sha256: instance.protocol_set_sha256.clone(),
            },
            credentials: ServerRuntimeCredentials {
                grant_id: instance.grant_id.clone(),
                grant_token: "A".repeat(43),
                runtime_instance_id: instance.runtime_instance_id.clone(),
                worker_id: instance.worker_id.clone(),
                session_token: "B".repeat(43),
            },
        }
    }
}
