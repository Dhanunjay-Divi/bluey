//! Best-effort local-to-cloud sync for sessions, transcript, answers, and RAG.
//!
//! The desktop remains local-first: every capture is written locally before
//! this module tries the managed cloud. Sync batches are idempotent and small
//! enough for the server's request limits, so retries are safe.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::{Context, Result};
use cue_cloud_client::{
    CloudChildTombstone, CloudClient, CloudSessionBundle, CloudSessionSummary,
    SessionAuditBundleResponse, SyncBatchRequest, SyncBatchResponse, SyncContextArtifactRecord,
    SyncCounts, SyncCueResponseRecord, SyncRagChunkRecord, SyncSessionRecord,
    SyncTranscriptSegment,
};
use cue_core::{
    app_paths::AppPaths, short_session_code, AnswerContextRole, CardArtifactType, ContextArtifact,
    ContextKind, ContextProcessingStatus, ConversationMemory, ConversationTurn, CueCardArtifact,
    MeetingDiagnostics, MeetingRecord, Speaker, TranscriptSegment,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::db::Database;
use crate::storage::MeetingStore;

const MAX_SYNC_RECORDS_PER_BATCH: usize = 450;
const MAX_TEXT_PREVIEW_CHARS: usize = 16_000;
const MAX_RESPONSE_CHARS: usize = 128_000;
const MAX_OBJECT_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;
const AUDIT_SCHEMA_VERSION: u32 = 1;
const AUDIT_BUNDLE_CONTENT_TYPE: &str = "application/json";
const MAX_AUDIT_BUNDLE_BYTES: usize = 25 * 1024 * 1024;
const MAX_AUDIT_EVENT_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_AUDIT_EVENT_RECORD_BYTES: usize = 64 * 1024;
const MAX_AUDIT_EVENT_RECORDS: u64 = 4_096;
const AUDIT_PRUNE_APPEND_BYTES: u64 = 4 * 1024 * 1024;
const AUDIT_EVENT_LOG_STATE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_AUDIT_LOCAL_RETENTION_DAYS: i64 = 7;
const DEFAULT_AUDIT_LOCAL_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SESSION_AUDIT_DIR: &str = "session-audit";
const SESSION_AUDIT_EVENTS_DIR: &str = "session-audit-events";
const SESSION_AUDIT_UPLOADED_DIR: &str = "session-audit-uploaded";
const SUPPORT_DIAGNOSTIC_UPLOAD_ENV: &str = "BLUEY_SUPPORT_DIAGNOSTIC_UPLOAD";
const MAX_SUPPORT_DIAGNOSTIC_BUNDLES_PER_SYNC: usize = 8;
const MAX_SUPPORT_DIAGNOSTIC_DIR_SCAN: usize = 512;
const CLOUD_SYNC_STATE_DIR: &str = "cloud-sync-state";
const CLOUD_DELETE_OUTBOX_DIR: &str = "cloud-delete-outbox";
const CLOUD_RESTORED_CONTEXT_DIR: &str = "cloud-restored-context";
const CLOUD_RESTORED_OBJECTS_DIR: &str = "cloud-restored-objects";
const CLOUD_HYDRATION_CURSOR_DIR: &str = "cloud-hydration-cursor";
const CLOUD_SYNC_STATE_SCHEMA_VERSION: u32 = 2;
const CLOUD_HYDRATION_CURSOR_SCHEMA_VERSION: u32 = 1;
const MAX_CLOUD_HYDRATION_PAGES_PER_RUN: usize = 8;
const CLOUD_DELETE_OUTBOX_SCHEMA_VERSION: u32 = 2;
const LEGACY_CLOUD_DELETE_OUTBOX_SCHEMA_VERSION: u32 = 1;
const CLOUD_DELETE_RETRY_BASE_MS: i64 = 5_000;
const CLOUD_DELETE_RETRY_MAX_MS: i64 = 60 * 60 * 1_000;
const CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION: u32 = 1;
const MAX_CLOUD_CONVERSATION_MEMORY_EPOCHS: usize = 8;
const MAX_CLOUD_CONVERSATION_MEMORY_EPOCH_CHARS: usize = 12_000;
const MAX_CLOUD_CONVERSATION_MEMORY_TIMESTAMP_CHARS: usize = 128;

type OperationGuard<'a> = dyn Fn() -> Result<()> + Sync + 'a;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncedObjectMetadata {
    object_key: String,
    size_bytes: u64,
    sha256: String,
    content_type: String,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudSyncState {
    schema_version: u32,
    owner_account_id: String,
    remote_session_id: String,
    #[serde(default)]
    remote_summary: Option<CloudSessionFingerprint>,
    #[serde(default = "empty_metadata")]
    session_metadata: Value,
    #[serde(default)]
    transcript_segments: BTreeMap<String, CloudTranscriptState>,
    #[serde(default)]
    context_artifacts: BTreeMap<String, CloudContextState>,
    #[serde(default)]
    responses: BTreeMap<String, CloudResponseState>,
    #[serde(default)]
    synced_transcript_records: BTreeMap<String, CloudTranscriptState>,
    #[serde(default)]
    synced_response_records: BTreeMap<String, CloudResponseState>,
    #[serde(default)]
    rag_chunks: BTreeMap<String, CloudRagState>,
    #[serde(default)]
    attachment_transfers: BTreeMap<String, CloudAttachmentTransferState>,
    #[serde(default)]
    child_tombstones: BTreeMap<String, CloudChildTombstone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudSessionFingerprint {
    title: String,
    status: String,
    updated_at_ms: i64,
    last_active_at_ms: Option<i64>,
    answer_style: Option<String>,
    transcript_count: i64,
    response_count: i64,
    context_count: i64,
    rag_count: i64,
    child_tombstone_count: i64,
    child_tombstone_updated_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudHydrationCursorState {
    schema_version: u32,
    owner_account_id: String,
    cursor: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CloudAttachmentTransferStatus {
    Synced,
    UploadRetry,
    DownloadRetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudAttachmentTransferState {
    record_id: String,
    status: CloudAttachmentTransferStatus,
    attempt_count: u32,
    updated_at_ms: i64,
    #[serde(default)]
    last_error_category: Option<String>,
    #[serde(default)]
    object: Option<SyncedObjectMetadata>,
}

#[derive(Debug, Clone, Default)]
struct AttachmentUploadSummary {
    uploaded: HashMap<Uuid, SyncedObjectMetadata>,
    retry_count: usize,
}

enum ArtifactObjectSource {
    NotApplicable,
    Available(PathBuf),
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingCloudSessionDelete {
    schema_version: u32,
    local_session_id: Uuid,
    remote_session_id: String,
    owner_account_id: String,
    queued_at_ms: i64,
    #[serde(default = "legacy_committed_cloud_delete_state")]
    state: CloudSessionDeleteState,
    #[serde(default)]
    committed_at_ms: Option<i64>,
    #[serde(default)]
    attempt_count: u32,
    #[serde(default)]
    next_retry_at_ms: i64,
    #[serde(default)]
    last_error_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEventLogState {
    schema_version: u32,
    last_sequence: u64,
    record_count: u64,
    bytes: u64,
}

impl Default for AuditEventLogState {
    fn default() -> Self {
        Self {
            schema_version: AUDIT_EVENT_LOG_STATE_SCHEMA_VERSION,
            last_sequence: 0,
            record_count: 0,
            bytes: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuditEventLogFingerprint {
    modified_at_ms: i64,
    bytes: u64,
    last_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CloudSessionDeleteState {
    Prepared,
    Committed,
}

fn legacy_committed_cloud_delete_state() -> CloudSessionDeleteState {
    // Version-one records were immediately flushable. Treating them as
    // committed preserves already-durable user delete requests during upgrade.
    CloudSessionDeleteState::Committed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudSessionDeleteDisposition {
    NotPreviouslyUploaded,
    Confirmed,
    Queued,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudTranscriptState {
    record_id: String,
    speaker: String,
    source: String,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    ts_ms: i64,
    #[serde(default = "empty_metadata")]
    metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudContextState {
    record_id: String,
    source_uri: Option<String>,
    content_hash: Option<String>,
    #[serde(default)]
    updated_at_ms: i64,
    #[serde(default = "empty_metadata")]
    metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudResponseState {
    record_id: String,
    kind: String,
    ts_ms: i64,
    model: Option<String>,
    lane: Option<String>,
    task_type: Option<String>,
    cost_cents: Option<i64>,
    balance_cents_after: Option<i64>,
    cost_label: Option<String>,
    #[serde(default = "empty_metadata")]
    metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudRagState {
    session_id: Option<String>,
    source_kind: String,
    source_id: String,
    chunk_index: i64,
    updated_at_ms: i64,
}

#[derive(Debug, Clone, Default)]
struct UploadedChildState {
    transcript_records: BTreeMap<String, CloudTranscriptState>,
    response_records: BTreeMap<String, CloudResponseState>,
    rag_chunks: BTreeMap<String, CloudRagState>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionAuditBundle {
    schema_version: u32,
    bundle_id: String,
    session_id: String,
    session_code: String,
    generated_at_ms: i64,
    content_policy: String,
    manifest: Value,
    events: Vec<Value>,
}

#[derive(Debug, Clone)]
struct BuiltAuditBundle {
    bundle: SessionAuditBundle,
    bytes: Vec<u8>,
    local_dir: PathBuf,
    event_log_fingerprint: Option<AuditEventLogFingerprint>,
}

#[derive(Debug, Clone)]
pub struct LocalSyncSummary {
    pub accepted: SyncCounts,
    pub batches: usize,
    pub server_time_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SessionAuditScope {
    pub session_id: Uuid,
    pub owner_account_id: Option<String>,
}

impl SessionAuditScope {
    pub(crate) fn from_meeting(meeting: &MeetingRecord) -> Self {
        Self {
            session_id: meeting.id,
            owner_account_id: meeting.owner_account_id.clone(),
        }
    }
}

/// Phase 624 diagnostic records deliberately omit raw account and device
/// identifiers. The authenticated support endpoint associates an upload with
/// its owner; the local diagnostic payload does not need a second identity
/// copy.
pub(crate) fn append_privacy_safe_diagnostic_events_for_scope(
    data_dir: &Path,
    scope: &SessionAuditScope,
    events: &[(String, Value)],
) -> Result<()> {
    append_session_audit_events_for_scope_inner(data_dir, scope, events)
}

fn append_session_audit_events_for_scope_inner(
    data_dir: &Path,
    scope: &SessionAuditScope,
    events: &[(String, Value)],
) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    if events.len() as u64 > MAX_AUDIT_EVENT_RECORDS {
        anyhow::bail!("local audit batch exceeds the bounded record count");
    }
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let event_dir = session_audit_event_dir(data_dir, scope);
    cue_core::app_paths::create_private_dir(&event_dir)?;
    let event_path = event_dir.join("events.jsonl");
    let mut event_state = load_audit_event_log_state(&event_path)?;
    let mut encoded_events = Vec::with_capacity(events.len());
    let mut incoming_bytes = 0_u64;
    for (offset, (kind, payload)) in events.iter().enumerate() {
        let sequence = event_state
            .last_sequence
            .saturating_add(offset as u64)
            .saturating_add(1);
        let encoded = encode_session_audit_event(scope, sequence, kind, payload.clone())?;
        incoming_bytes = incoming_bytes.saturating_add(encoded.len() as u64);
        encoded_events.push(encoded);
    }
    if incoming_bytes > MAX_AUDIT_EVENT_LOG_BYTES {
        anyhow::bail!("local audit batch exceeds the bounded event log size");
    }
    let incoming_records = encoded_events.len() as u64;
    if event_state.record_count.saturating_add(incoming_records) > MAX_AUDIT_EVENT_RECORDS
        || event_state.bytes.saturating_add(incoming_bytes) > MAX_AUDIT_EVENT_LOG_BYTES
    {
        event_state = compact_audit_event_log(&event_path, incoming_bytes, incoming_records)?;
    }

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&event_path)
        .with_context(|| format!("open {}", event_path.display()))?;
    for encoded in &encoded_events {
        file.write_all(encoded)
            .with_context(|| format!("write {}", event_path.display()))?;
    }
    file.sync_data()
        .with_context(|| format!("sync {}", event_path.display()))?;
    drop(file);
    event_state.last_sequence = event_state.last_sequence.saturating_add(incoming_records);
    event_state.record_count = event_state.record_count.saturating_add(incoming_records);
    event_state.bytes = event_state.bytes.saturating_add(incoming_bytes);
    write_audit_event_log_state(&event_path, &event_state)?;
    if audit_append_should_prune(incoming_bytes) {
        // Global retention cannot depend on cloud sync, which is opt-in. The
        // first append in each process performs a recovery scan, then scans are
        // amortized by durable bytes appended so UI event throughput stays O(1)
        // in the steady state while total local storage remains capped.
        prune_local_audit_storage(data_dir);
    }
    Ok(())
}

fn encode_session_audit_event(
    scope: &SessionAuditScope,
    sequence: u64,
    kind: &str,
    payload: Value,
) -> Result<Vec<u8>> {
    let session_code = short_session_code(scope.session_id);
    let event_id = format!(
        "{session_code}-ui-{sequence:08}-{}",
        Uuid::new_v4().simple()
    );
    let mut record = json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "event_id": event_id,
        "session_id": scope.session_id.to_string(),
        "session_code": session_code,
        "sequence": sequence,
        "kind": kind,
        "created_at_ms": current_epoch_ms(),
        "source": "desktop_ui",
        "payload": support_diagnostic_payload(payload),
    });
    let mut encoded = serde_json::to_vec(&record).context("serialize local audit event")?;
    if encoded.len().saturating_add(1) > MAX_AUDIT_EVENT_RECORD_BYTES {
        record["payload"] = json!({
            "content_policy": "metadata_only",
            "payload_redacted": true,
            "reason_code": "record_size_cap",
        });
        encoded = serde_json::to_vec(&record).context("serialize bounded local audit event")?;
    }
    encoded.push(b'\n');
    if encoded.len() > MAX_AUDIT_EVENT_RECORD_BYTES {
        anyhow::bail!("local audit event exceeds the bounded record size");
    }
    Ok(encoded)
}

fn audit_append_should_prune(incoming_bytes: u64) -> bool {
    static FIRST_APPEND: AtomicBool = AtomicBool::new(true);
    static BYTES_SINCE_PRUNE: AtomicU64 = AtomicU64::new(0);
    if FIRST_APPEND.swap(false, Ordering::AcqRel) {
        BYTES_SINCE_PRUNE.store(0, Ordering::Release);
        return true;
    }
    let accumulated = BYTES_SINCE_PRUNE
        .fetch_add(incoming_bytes, Ordering::AcqRel)
        .saturating_add(incoming_bytes);
    if accumulated >= AUDIT_PRUNE_APPEND_BYTES {
        BYTES_SINCE_PRUNE.store(0, Ordering::Release);
        true
    } else {
        false
    }
}

/// Diagnostic bundles deliberately contain operational metadata only. The
/// product session sync already owns questions, answers, transcripts, and
/// attachments; duplicating those values into diagnostics creates a second,
/// harder-to-govern copy of private customer content.
impl LocalSyncSummary {
    fn empty() -> Self {
        Self {
            accepted: SyncCounts {
                sessions: 0,
                transcript_segments: 0,
                cue_responses: 0,
                context_artifacts: 0,
                rag_chunks: 0,
            },
            batches: 0,
            server_time_ms: None,
        }
    }

    fn add_response(&mut self, response: SyncBatchResponse) {
        self.accepted.sessions += response.accepted.sessions;
        self.accepted.transcript_segments += response.accepted.transcript_segments;
        self.accepted.cue_responses += response.accepted.cue_responses;
        self.accepted.context_artifacts += response.accepted.context_artifacts;
        self.accepted.rag_chunks += response.accepted.rag_chunks;
        self.batches += 1;
        self.server_time_ms = Some(response.server_time_ms);
    }

    pub fn total_records(&self) -> usize {
        self.accepted.sessions
            + self.accepted.transcript_segments
            + self.accepted.cue_responses
            + self.accepted.context_artifacts
            + self.accepted.rag_chunks
    }
}

#[derive(Debug, Clone, Default)]
pub struct CloudHydrationSummary {
    pub restored_sessions: usize,
    pub skipped_sessions: usize,
    pub purged_deleted_sessions: usize,
    pub purged_session_ids: Vec<Uuid>,
    pub reconciled_session_ids: Vec<Uuid>,
    pub attachment_retry_count: usize,
    pub local_cleanup_retry_count: usize,
    pub continuation_pending: bool,
}

impl CloudHydrationSummary {
    pub fn total_sessions(&self) -> usize {
        self.restored_sessions + self.skipped_sessions + self.purged_deleted_sessions
    }

    fn record_purged_session_id(&mut self, seen_session_ids: &mut HashSet<Uuid>, session_id: Uuid) {
        if seen_session_ids.insert(session_id) {
            self.purged_session_ids.push(session_id);
        }
    }
}

pub async fn sync_local_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<LocalSyncSummary> {
    operation_is_active()?;
    let owner_account_id = required_owner_account_id(owner_account_id)?;
    reconcile_prepared_cloud_session_deletes(data_dir, store, Some(owner_account_id))
        .context("reconcile interrupted local session deletions")?;
    flush_pending_cloud_session_deletes(
        data_dir,
        client,
        Some(owner_account_id),
        operation_is_active,
    )
    .await?;
    let cloud_delete_retry_pending =
        has_committed_cloud_session_delete(data_dir, owner_account_id)?;
    operation_is_active()?;
    let meetings = store
        .all_meetings()
        .context("failed to load local sessions")?
        .into_iter()
        .filter(|meeting| meeting_belongs_to_owner(meeting, Some(owner_account_id)))
        .collect::<Vec<_>>();
    if meetings.is_empty() {
        if cloud_delete_retry_pending {
            anyhow::bail!("cloud session deletion remains queued for retry");
        }
        return Ok(LocalSyncSummary::empty());
    }

    let response_map = load_local_responses(data_dir, owner_account_id, &meetings);
    let mut sync_states = load_cloud_sync_states(data_dir, owner_account_id, &meetings);
    for parent_batch in build_object_parent_batches(&meetings, &response_map, &sync_states) {
        operation_is_active()?;
        client
            .sync_batch(&parent_batch)
            .await
            .context("reserve cloud parent sessions before object upload")?;
        operation_is_active()?;
    }
    let uploaded_objects = upload_context_objects(
        data_dir,
        owner_account_id,
        &meetings,
        &response_map,
        &mut sync_states,
        client,
        operation_is_active,
    )
    .await?;
    let batches = build_sync_batches_with_states(
        &meetings,
        &response_map,
        &uploaded_objects.uploaded,
        &sync_states,
    );
    if batches.is_empty() {
        if uploaded_objects.retry_count > 0 {
            anyhow::bail!(
                "cloud attachment sync remains incomplete; {} object(s) require retry",
                uploaded_objects.retry_count
            );
        }
        if cloud_delete_retry_pending {
            anyhow::bail!("cloud session deletion remains queued for retry");
        }
        return Ok(LocalSyncSummary::empty());
    }
    let uploaded_child_states = uploaded_child_states_by_session(&batches);

    let mut summary = LocalSyncSummary::empty();
    for batch in batches {
        operation_is_active()?;
        let response = client
            .sync_batch(&batch)
            .await
            .context("cloud sync batch")?;
        operation_is_active()?;
        summary.add_response(response);
    }
    for meeting in &meetings {
        operation_is_active()?;
        let responses = response_map.get(&meeting.id.to_string()).map(Vec::as_slice);
        let previous = sync_states.get(&meeting.id);
        let wire_session_id = wire_session_id(meeting, previous);
        let state = cloud_sync_state_after_upload(
            meeting,
            responses,
            &uploaded_objects.uploaded,
            previous,
            uploaded_child_states.get(&wire_session_id),
            owner_account_id,
        );
        write_cloud_sync_state(data_dir, owner_account_id, meeting.id, &state)?;
    }
    if uploaded_objects.retry_count > 0 {
        anyhow::bail!(
            "cloud attachment sync remains incomplete; {} object(s) require retry",
            uploaded_objects.retry_count
        );
    }
    if cloud_delete_retry_pending {
        anyhow::bail!("cloud session deletion remains queued for retry");
    }
    Ok(summary)
}

fn support_diagnostic_upload_enabled(explicit_user_consent: bool) -> bool {
    explicit_user_consent
        && std::env::var(SUPPORT_DIAGNOSTIC_UPLOAD_ENV)
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(true)
}

/// Upload metadata-only support diagnostics independently of product session
/// sync. The explicit persisted user-consent bit is mandatory; an operator env
/// value may disable this path but cannot enable it on the user's behalf.
pub async fn sync_support_diagnostic_bundles(
    store: &MeetingStore,
    paths: &AppPaths,
    client: &CloudClient,
    owner_account_id: Option<&str>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<usize> {
    let owner_account_id = required_owner_account_id(owner_account_id)?;
    let consent_is_active = || {
        if operation_is_active().is_err() {
            return false;
        }
        cue_core::load_settings(paths)
            .map(|settings| {
                support_diagnostic_upload_enabled(
                    settings.support_diagnostics_upload_allowed_for_account(Some(owner_account_id)),
                )
            })
            .unwrap_or(false)
    };
    if !consent_is_active() {
        prune_local_audit_storage(&paths.data_dir);
        return Ok(0);
    }
    let _guard = support_diagnostic_sync_lock().lock().await;
    let store = store.clone();
    let data_dir = paths.data_dir.clone();
    let worker_owner_account_id = owner_account_id.to_string();
    let meetings = tokio::task::spawn_blocking(move || -> Result<Vec<MeetingRecord>> {
        let mut meetings = Vec::new();
        for session_id in
            pending_support_diagnostic_session_ids(&data_dir, &worker_owner_account_id)?
        {
            let Some(meeting) = store.load_by_id(session_id)? else {
                continue;
            };
            if meeting_belongs_to_owner(&meeting, Some(&worker_owner_account_id)) {
                meetings.push(meeting);
            }
        }
        Ok(meetings)
    })
    .await
    .context("join support diagnostic session lookup")??;
    if meetings.is_empty() {
        return Ok(0);
    }
    if !consent_is_active() {
        return Ok(0);
    }
    operation_is_active()?;
    let server_consent = client
        .get_support_diagnostic_consent()
        .await
        .map_err(|_| anyhow::anyhow!("support diagnostic server consent check deferred"))?;
    operation_is_active()?;
    let server_revision = server_consent
        .current_receipt
        .as_ref()
        .map(|receipt| receipt.revision)
        .filter(|revision| *revision > 0);
    if let Some(server_revision) = server_revision {
        crate::diagnostics::align_support_diagnostic_server_revision(
            &paths.data_dir,
            owner_account_id,
            server_revision,
        )?;
    }
    if server_revision.is_none()
        || !server_consent.enabled
        || server_consent.policy_version != cue_cloud_client::SUPPORT_DIAGNOSTIC_POLICY_VERSION
        || server_consent.content_policy != cue_cloud_client::SUPPORT_DIAGNOSTIC_CONTENT_POLICY
    {
        crate::diagnostics::fence_support_diagnostics_for_owner(&paths.data_dir, owner_account_id)?;
        let _ = cue_core::update_settings(paths, |settings| {
            settings.support_diagnostics_upload_enabled = false;
            settings.support_diagnostics_upload_consent_granted = false;
        });
        return Ok(0);
    }
    sync_session_audit_bundles(&paths.data_dir, &meetings, client, consent_is_active).await
}

fn support_diagnostic_sync_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn pending_support_diagnostic_session_ids(
    data_dir: &Path,
    owner_account_id: &str,
) -> Result<Vec<Uuid>> {
    let root = data_dir
        .join(SESSION_AUDIT_EVENTS_DIR)
        .join(account_scope_key(owner_account_id)?);
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut pending = entries
        .take(MAX_SUPPORT_DIAGNOSTIC_DIR_SCAN)
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let session_id = entry
                .file_name()
                .to_str()
                .and_then(|value| Uuid::parse_str(value).ok())?;
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((modified, session_id))
        })
        .collect::<Vec<_>>();
    pending.sort_by_key(|(modified, _)| *modified);
    Ok(pending
        .into_iter()
        .take(MAX_SUPPORT_DIAGNOSTIC_BUNDLES_PER_SYNC)
        .map(|(_, session_id)| session_id)
        .collect())
}

pub fn prepare_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<CloudSessionDeleteDisposition> {
    let remote_session_id = load_cloud_sync_state(data_dir, owner_account_id, local_session_id)
        .map(|state| state.remote_session_id)
        // A server may have accepted the idempotent session write immediately
        // before a crash prevented the provenance sidecar from being written.
        // Conservatively queue the canonical local UUID for owned sessions.
        .unwrap_or_else(|| local_session_id.to_string());
    let pending = PendingCloudSessionDelete {
        schema_version: CLOUD_DELETE_OUTBOX_SCHEMA_VERSION,
        local_session_id,
        remote_session_id,
        owner_account_id: owner_account_id.to_string(),
        queued_at_ms: current_epoch_ms(),
        state: CloudSessionDeleteState::Prepared,
        committed_at_ms: None,
        attempt_count: 0,
        next_retry_at_ms: 0,
        last_error_category: None,
    };
    write_pending_cloud_delete(data_dir, &pending)?;
    Ok(CloudSessionDeleteDisposition::Queued)
}

pub fn commit_prepared_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<CloudSessionDeleteDisposition> {
    let path = cloud_delete_outbox_path(data_dir, owner_account_id, local_session_id)?;
    let mut pending = read_cloud_delete_intent(&path)?.with_context(|| {
        format!("prepared cloud deletion for session {local_session_id} missing")
    })?;
    validate_cloud_delete_intent(&pending, local_session_id, owner_account_id)?;
    if pending.state == CloudSessionDeleteState::Committed {
        return Ok(CloudSessionDeleteDisposition::Queued);
    }
    pending.state = CloudSessionDeleteState::Committed;
    pending.committed_at_ms = Some(current_epoch_ms());
    pending.attempt_count = 0;
    pending.next_retry_at_ms = 0;
    pending.last_error_category = None;
    write_pending_cloud_delete(data_dir, &pending)?;
    Ok(CloudSessionDeleteDisposition::Queued)
}

/// Resolve the only ambiguous point in the local/cloud delete transaction.
///
/// A prepared intent with a canonical local record means local deletion never
/// committed, so it is cancelled. If the same account owns the intent and the
/// canonical record is absent, local deletion committed and the intent is
/// promoted for an idempotent cloud retry. Other-account records remain inert.
pub fn reconcile_prepared_cloud_session_deletes(
    data_dir: &Path,
    store: &MeetingStore,
    owner_account_id: Option<&str>,
) -> Result<()> {
    let _transaction_guard = cloud_session_delete_transaction_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reconcile_prepared_cloud_session_deletes_locked(data_dir, store, owner_account_id)
}

fn reconcile_prepared_cloud_session_deletes_locked(
    data_dir: &Path,
    store: &MeetingStore,
    owner_account_id: Option<&str>,
) -> Result<()> {
    let Some(owner_account_id) = owner_account_id
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
    else {
        return Ok(());
    };
    migrate_legacy_cloud_delete_intents(data_dir, owner_account_id)?;
    let dir = cloud_delete_outbox_dir(data_dir, owner_account_id)?;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten().take(256) {
        let path = entry.path();
        let Some(local_session_id) = cloud_delete_outbox_session_id(&path) else {
            continue;
        };
        let Some(mut pending) = read_cloud_delete_intent(&path)? else {
            continue;
        };
        if pending.state != CloudSessionDeleteState::Prepared
            || validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_err()
        {
            continue;
        }

        if let Some(meeting) = store.load_by_id(local_session_id)? {
            if meeting_belongs_to_owner(&meeting, Some(owner_account_id)) {
                abort_prepared_cloud_session_delete(data_dir, local_session_id, owner_account_id)?;
            }
            continue;
        }

        pending.state = CloudSessionDeleteState::Committed;
        pending.committed_at_ms = Some(current_epoch_ms());
        pending.attempt_count = 0;
        pending.next_retry_at_ms = 0;
        pending.last_error_category = None;
        write_pending_cloud_delete(data_dir, &pending)?;
    }
    Ok(())
}

/// Hold this guard from writing a prepared delete intent through the local
/// MeetingStore deletion and durable commit/abort transition. Reconciliation
/// uses the same process-wide lock, so it cannot misclassify an in-flight local
/// deletion as an abandoned prepare.
pub fn lock_cloud_session_delete_transaction() -> MutexGuard<'static, ()> {
    cloud_session_delete_transaction_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cloud_session_delete_transaction_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn abort_prepared_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<()> {
    abort_prepared_cloud_session_delete_with(data_dir, local_session_id, owner_account_id, |path| {
        fs::remove_file(path)
    })
}

fn abort_prepared_cloud_session_delete_with<F>(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
    remove: F,
) -> Result<()>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    let path = cloud_delete_outbox_path(data_dir, owner_account_id, local_session_id)?;
    let Some(pending) = read_cloud_delete_intent(&path)? else {
        return Ok(());
    };
    validate_cloud_delete_intent(&pending, local_session_id, owner_account_id)?;
    if pending.state != CloudSessionDeleteState::Prepared {
        anyhow::bail!("refusing to abort committed cloud deletion for session {local_session_id}");
    }
    match remove(&path) {
        Ok(()) => sync_deleted_private_file_parent(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("remove prepared cloud deletion {}", path.display()))
        }
    }
}

pub async fn flush_queued_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    client: &CloudClient,
    owner_account_id: &str,
    operation_is_active: &OperationGuard<'_>,
) -> Result<CloudSessionDeleteDisposition> {
    flush_queued_cloud_session_delete_with(
        data_dir,
        local_session_id,
        client,
        owner_account_id,
        operation_is_active,
    )
    .await
}

#[async_trait::async_trait]
trait CloudSessionDeleteClient {
    async fn delete_cloud_session(
        &self,
        remote_session_id: &str,
    ) -> std::result::Result<(), cue_cloud_client::Error>;
}

#[async_trait::async_trait]
impl CloudSessionDeleteClient for CloudClient {
    async fn delete_cloud_session(
        &self,
        remote_session_id: &str,
    ) -> std::result::Result<(), cue_cloud_client::Error> {
        CloudClient::delete_cloud_session(self, remote_session_id)
            .await
            .map(|_| ())
    }
}

async fn flush_queued_cloud_session_delete_with<C>(
    data_dir: &Path,
    local_session_id: Uuid,
    client: &C,
    owner_account_id: &str,
    operation_is_active: &OperationGuard<'_>,
) -> Result<CloudSessionDeleteDisposition>
where
    C: CloudSessionDeleteClient + Sync + ?Sized,
{
    let path = cloud_delete_outbox_path(data_dir, owner_account_id, local_session_id)?;
    let Some(mut pending) = read_cloud_delete_intent(&path).ok().flatten() else {
        return Ok(CloudSessionDeleteDisposition::NotPreviouslyUploaded);
    };
    if validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_err() {
        return Ok(CloudSessionDeleteDisposition::NotPreviouslyUploaded);
    }
    if pending.state != CloudSessionDeleteState::Committed {
        return Ok(CloudSessionDeleteDisposition::Queued);
    }
    if pending.next_retry_at_ms > current_epoch_ms() {
        return Ok(CloudSessionDeleteDisposition::Queued);
    }
    operation_is_active()?;
    match client
        .delete_cloud_session(&pending.remote_session_id)
        .await
    {
        Ok(_) => {
            operation_is_active()?;
            remove_cloud_delete_provenance(data_dir, owner_account_id, local_session_id)?;
            Ok(CloudSessionDeleteDisposition::Confirmed)
        }
        Err(error) => {
            operation_is_active()?;
            let error_category = cloud_delete_error_category(&error);
            schedule_cloud_delete_retry(data_dir, &mut pending, error_category)?;
            warn!(
                session_hash = %cloud_log_identifier_hash(local_session_id.as_bytes()),
                error_category,
                attempt_count = pending.attempt_count,
                "cloud session deletion remains queued"
            );
            Ok(CloudSessionDeleteDisposition::Queued)
        }
    }
}

pub async fn flush_pending_cloud_session_deletes(
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<()> {
    flush_pending_cloud_session_deletes_with(
        data_dir,
        client,
        owner_account_id,
        operation_is_active,
    )
    .await
}

async fn flush_pending_cloud_session_deletes_with<C>(
    data_dir: &Path,
    client: &C,
    owner_account_id: Option<&str>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<()>
where
    C: CloudSessionDeleteClient + Sync + ?Sized,
{
    let Some(owner_account_id) = owner_account_id
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
    else {
        return Ok(());
    };
    migrate_legacy_cloud_delete_intents(data_dir, owner_account_id)?;
    let dir = cloud_delete_outbox_dir(data_dir, owner_account_id)?;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten().take(256) {
        let path = entry.path();
        let Some(local_session_id) = cloud_delete_outbox_session_id(&path) else {
            continue;
        };
        let Some(mut pending) = read_cloud_delete_intent(&path).ok().flatten() else {
            continue;
        };
        if validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_err()
            || pending.state != CloudSessionDeleteState::Committed
        {
            continue;
        }
        if pending.next_retry_at_ms > current_epoch_ms() {
            continue;
        }
        operation_is_active()?;
        match client
            .delete_cloud_session(&pending.remote_session_id)
            .await
        {
            Ok(_) => {
                operation_is_active()?;
                remove_cloud_delete_provenance(data_dir, owner_account_id, local_session_id)?;
            }
            Err(error) => {
                operation_is_active()?;
                let error_category = cloud_delete_error_category(&error);
                schedule_cloud_delete_retry(data_dir, &mut pending, error_category)?;
                debug!(
                    session_hash = %cloud_log_identifier_hash(local_session_id.as_bytes()),
                    error_category,
                    attempt_count = pending.attempt_count,
                    "pending cloud session deletion remains queued"
                );
            }
        }
    }
    Ok(())
}

fn cloud_delete_outbox_session_id(path: &Path) -> Option<Uuid> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
        return None;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Uuid::parse_str(stem).ok())
}

fn migrate_legacy_cloud_delete_intents(data_dir: &Path, owner_account_id: &str) -> Result<()> {
    let legacy_root = data_dir.join(CLOUD_DELETE_OUTBOX_DIR);
    let Ok(entries) = fs::read_dir(&legacy_root) else {
        return Ok(());
    };
    for entry in entries.flatten().take(256) {
        let legacy_path = entry.path();
        if !legacy_path.is_file() {
            continue;
        }
        let Some(session_id) = cloud_delete_outbox_session_id(&legacy_path) else {
            continue;
        };
        let Some(pending) = read_cloud_delete_intent(&legacy_path)? else {
            continue;
        };
        if validate_cloud_delete_intent(&pending, session_id, owner_account_id).is_err() {
            continue;
        }
        let scoped_path = cloud_delete_outbox_path(data_dir, owner_account_id, session_id)?;
        if let Some(existing) = read_cloud_delete_intent(&scoped_path)? {
            if existing != pending {
                continue;
            }
        } else {
            write_pending_cloud_delete(data_dir, &pending)?;
        }
        match fs::remove_file(&legacy_path) {
            Ok(()) => sync_deleted_private_file_parent(&legacy_path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).context("remove migrated legacy cloud deletion intent");
            }
        }
    }
    Ok(())
}

fn read_cloud_delete_intent(path: &Path) -> Result<Option<PendingCloudSessionDelete>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("read cloud deletion intent {}", path.display()));
        }
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parse cloud deletion intent {}", path.display()))
        .map(Some)
}

fn cloud_session_has_committed_delete(
    data_dir: &Path,
    owner_account_id: &str,
    local_session_id: Uuid,
) -> Result<bool> {
    let path = cloud_delete_outbox_path(data_dir, owner_account_id, local_session_id)?;
    let Some(pending) = read_cloud_delete_intent(&path)? else {
        return Ok(false);
    };
    validate_cloud_delete_intent(&pending, local_session_id, owner_account_id)?;
    Ok(pending.state == CloudSessionDeleteState::Committed)
}

fn has_committed_cloud_session_delete(data_dir: &Path, owner_account_id: &str) -> Result<bool> {
    let dir = cloud_delete_outbox_dir(data_dir, owner_account_id)?;
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(false);
    };
    for (index, entry) in entries.flatten().enumerate() {
        if index >= 512 {
            // A pathological outbox must not be reported as fully synced just
            // because the bounded scan stopped before a committed record.
            return Ok(true);
        }
        let path = entry.path();
        let Some(local_session_id) = cloud_delete_outbox_session_id(&path) else {
            continue;
        };
        let Some(pending) = read_cloud_delete_intent(&path)? else {
            continue;
        };
        if validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_ok()
            && pending.state == CloudSessionDeleteState::Committed
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_cloud_delete_intent(
    pending: &PendingCloudSessionDelete,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<()> {
    if !matches!(
        pending.schema_version,
        LEGACY_CLOUD_DELETE_OUTBOX_SCHEMA_VERSION | CLOUD_DELETE_OUTBOX_SCHEMA_VERSION
    ) {
        anyhow::bail!(
            "unsupported cloud deletion intent schema {}",
            pending.schema_version
        );
    }
    if pending.local_session_id != local_session_id {
        anyhow::bail!("cloud deletion intent session identity mismatch");
    }
    if pending.owner_account_id != owner_account_id {
        anyhow::bail!("cloud deletion intent account owner mismatch");
    }
    if pending.remote_session_id.trim().is_empty() {
        anyhow::bail!("cloud deletion intent remote session id is empty");
    }
    Ok(())
}

fn cloud_delete_error_category(error: &cue_cloud_client::Error) -> &'static str {
    match error {
        cue_cloud_client::Error::Unauthorized => "authentication",
        cue_cloud_client::Error::RateLimited { .. } => "rate_limited",
        cue_cloud_client::Error::Server { .. } => "server",
        cue_cloud_client::Error::Network(_) => "network",
        _ => "client",
    }
}

fn cloud_delete_retry_delay_ms(local_session_id: Uuid, attempt_count: u32) -> i64 {
    let exponent = attempt_count.saturating_sub(1).min(16);
    let base_cap = CLOUD_DELETE_RETRY_MAX_MS.saturating_mul(4) / 5;
    let base = CLOUD_DELETE_RETRY_BASE_MS
        .saturating_mul(1_i64.checked_shl(exponent).unwrap_or(i64::MAX))
        .min(base_cap);
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-cloud-delete-retry-v1\0");
    hasher.update(local_session_id.as_bytes());
    hasher.update(attempt_count.to_be_bytes());
    let digest = hasher.finalize();
    let jitter_bucket = u16::from_be_bytes([digest[0], digest[1]]) as i64;
    let jitter = (base / 4).saturating_mul(jitter_bucket) / i64::from(u16::MAX);
    base.saturating_add(jitter).min(CLOUD_DELETE_RETRY_MAX_MS)
}

fn schedule_cloud_delete_retry(
    data_dir: &Path,
    pending: &mut PendingCloudSessionDelete,
    error_category: &str,
) -> Result<()> {
    pending.attempt_count = pending.attempt_count.saturating_add(1);
    pending.next_retry_at_ms = current_epoch_ms().saturating_add(cloud_delete_retry_delay_ms(
        pending.local_session_id,
        pending.attempt_count,
    ));
    pending.last_error_category = Some(error_category.to_string());
    write_pending_cloud_delete(data_dir, pending)
}

pub async fn hydrate_missing_cloud_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
    limit: i64,
    operation_is_active: &OperationGuard<'_>,
    persistence_barrier: &AsyncMutex<()>,
) -> Result<CloudHydrationSummary> {
    let owner_account_id = required_owner_account_id(owner_account_id)?;
    reconcile_prepared_cloud_session_deletes(data_dir, store, Some(owner_account_id))
        .context("reconcile interrupted local session deletions before hydration")?;
    let mut summary = CloudHydrationSummary::default();
    let mut cursor = load_cloud_hydration_cursor(data_dir, owner_account_id);
    let mut seen_cursors = HashSet::new();
    if let Some(cursor) = cursor.as_ref() {
        seen_cursors.insert(cursor.clone());
    }
    let mut seen_purged_session_ids = HashSet::new();
    let mut seen_reconciled_session_ids = HashSet::new();
    let mut processed_pages = 0usize;
    loop {
        let attachment_retries_before_page = summary.attachment_retry_count;
        let cleanup_retries_before_page = summary.local_cleanup_retry_count;
        operation_is_active()?;
        let response = client
            .list_cloud_sessions_page(Some(limit.clamp(1, 200)), cursor.as_deref())
            .await
            .context("list cloud sessions")?;
        operation_is_active()?;
        let next_cursor = response.next_cursor.clone();

        for deleted in response.deleted_sessions {
            operation_is_active()?;
            let session_id =
                local_uuid_for_cloud_id("session", "account-session", &deleted.session_id);
            // Surface every account-scoped server tombstone to the daemon even
            // when its MeetingStore file is already absent. Other local views
            // (the active in-memory session, projections, RAG, or diagnostics)
            // may still need deterministic cleanup.
            summary.record_purged_session_id(&mut seen_purged_session_ids, session_id);
            if purge_cloud_session_local_state(data_dir, owner_account_id, session_id).is_err() {
                summary.local_cleanup_retry_count =
                    summary.local_cleanup_retry_count.saturating_add(1);
                debug!("account-scoped cloud tombstone cache cleanup remains queued for retry");
            }
            let _persistence = persistence_barrier.lock().await;
            operation_is_active()?;
            let Some(local_meeting) = store.load_by_id(session_id)? else {
                continue;
            };
            if !meeting_should_follow_cloud_delete(&local_meeting, Some(owner_account_id)) {
                summary.skipped_sessions += 1;
                continue;
            }
            operation_is_active()?;
            remove_bluey_owned_context_files(data_dir, &local_meeting);
            if store.delete(session_id)? {
                summary.purged_deleted_sessions += 1;
            }
        }

        for session in response.sessions {
            operation_is_active()?;
            let session_id =
                local_uuid_for_cloud_id("session", "account-session", &session.session_id);
            if cloud_session_has_committed_delete(data_dir, owner_account_id, session_id)? {
                summary.skipped_sessions += 1;
                continue;
            }
            let existing = store.load_by_id(session_id)?;
            if existing
                .as_ref()
                .is_some_and(|meeting| !meeting_belongs_to_owner(meeting, Some(owner_account_id)))
            {
                summary.skipped_sessions += 1;
                continue;
            }
            if existing.is_some()
                && cloud_session_summary_is_current(
                    data_dir,
                    owner_account_id,
                    session_id,
                    &session,
                )
            {
                summary.skipped_sessions += 1;
                continue;
            }

            operation_is_active()?;
            let bundle = client
                .load_cloud_session(&session.session_id)
                .await
                .with_context(|| format!("load cloud session {}", session.session_id))?;
            operation_is_active()?;
            if bundle.session.session_id != session.session_id {
                anyhow::bail!(
                    "cloud session list/bundle identity mismatch: expected {}, got {}",
                    session.session_id,
                    bundle.session.session_id
                );
            }
            let (mut cloud_meeting, attachment_retry_count, child_tombstones) =
                meeting_from_cloud_bundle(
                    data_dir,
                    Some(client),
                    owner_account_id,
                    bundle,
                    operation_is_active,
                )
                .await?;
            operation_is_active()?;
            cloud_meeting.owner_account_id = Some(owner_account_id.to_string());
            summary.attachment_retry_count = summary
                .attachment_retry_count
                .saturating_add(attachment_retry_count);
            if !child_tombstones.is_empty() && seen_reconciled_session_ids.insert(session_id) {
                summary.reconciled_session_ids.push(session_id);
            }
            // Re-read and publish the local mutation under the same barrier as
            // live STT, final answers, sign-out, and account deletion. The
            // earlier copy was only an optimization hint; it cannot authorize
            // a write after network I/O.
            let _persistence = persistence_barrier.lock().await;
            operation_is_active()?;
            let existing = store.load_by_id(session_id)?;
            if existing
                .as_ref()
                .is_some_and(|meeting| !meeting_belongs_to_owner(meeting, Some(owner_account_id)))
            {
                summary.skipped_sessions += 1;
                continue;
            }
            if let Some(existing) = existing.as_ref() {
                operation_is_active()?;
                remove_cloud_tombstoned_context_files(
                    data_dir,
                    owner_account_id,
                    existing,
                    &child_tombstones,
                );
            }
            purge_cloud_response_tombstones(
                data_dir,
                owner_account_id,
                cloud_meeting.id,
                &child_tombstones,
            )?;
            if !meeting_has_syncable_content(&cloud_meeting, None)
                && (existing.is_none() || child_tombstones.is_empty())
            {
                mark_cloud_session_summary_current(
                    data_dir,
                    owner_account_id,
                    session_id,
                    &session,
                )?;
                summary.skipped_sessions += 1;
                continue;
            }

            if let Some(existing) = existing {
                let active = store
                    .load_active()?
                    .is_some_and(|meeting| meeting.id == existing.id);
                let (meeting, changed) =
                    reconcile_cloud_meeting(existing, cloud_meeting, &child_tombstones)?;
                if !changed {
                    mark_cloud_session_summary_current(
                        data_dir,
                        owner_account_id,
                        session_id,
                        &session,
                    )?;
                    summary.skipped_sessions += 1;
                    continue;
                }
                operation_is_active()?;
                if active {
                    store.save_active(&meeting)?;
                } else {
                    store.save_archived(&meeting)?;
                }
            } else {
                operation_is_active()?;
                store.save_archived(&cloud_meeting)?;
            }
            mark_cloud_session_summary_current(data_dir, owner_account_id, session_id, &session)?;
            summary.restored_sessions += 1;
        }

        if summary.attachment_retry_count > attachment_retries_before_page
            || summary.local_cleanup_retry_count > cleanup_retries_before_page
        {
            // Do not checkpoint past a page whose attachment or local purge
            // side effects are incomplete. The next bounded retry resumes this
            // exact page, so a partial first page cannot hide behind a cursor
            // while later pages make the account appear current.
            break;
        }
        processed_pages = processed_pages.saturating_add(1);
        if !advance_cloud_session_cursor(&mut cursor, &mut seen_cursors, next_cursor)? {
            clear_cloud_hydration_cursor(data_dir, owner_account_id)?;
            break;
        }
        write_cloud_hydration_cursor(
            data_dir,
            owner_account_id,
            cursor
                .as_deref()
                .context("cloud hydration cursor missing")?,
        )?;
        if processed_pages >= MAX_CLOUD_HYDRATION_PAGES_PER_RUN {
            summary.continuation_pending = true;
            break;
        }
    }
    Ok(summary)
}

/// Reapply the latest durable server child tombstones to a stale in-memory
/// meeting immediately before it is saved. This closes the small window
/// between hydration publishing deletion provenance and the runtime reloading
/// its active MeetingRecord.
pub(crate) fn reapply_cloud_child_tombstones_before_save(
    data_dir: &Path,
    owner_account_id: &str,
    meeting: &mut MeetingRecord,
) -> Result<usize> {
    let owner_account_id = required_owner_account_id(Some(owner_account_id))?;
    anyhow::ensure!(
        meeting.owner_account_id.as_deref() == Some(owner_account_id),
        "refusing to apply cloud tombstones across account owners"
    );
    let Some(state) =
        load_cloud_sync_state_strict_if_present(data_dir, owner_account_id, meeting.id)?
    else {
        return Ok(0);
    };
    let before = serde_json::to_value(&*meeting)?;
    let tombstones = state.child_tombstones.values().cloned().collect::<Vec<_>>();
    apply_cloud_child_tombstones_to_meeting(meeting, &tombstones);
    Ok(usize::from(before != serde_json::to_value(&*meeting)?))
}

fn advance_cloud_session_cursor(
    cursor: &mut Option<String>,
    seen_cursors: &mut HashSet<String>,
    next_cursor: Option<String>,
) -> Result<bool> {
    let Some(next_cursor) = next_cursor else {
        return Ok(false);
    };
    if !seen_cursors.insert(next_cursor.clone()) {
        anyhow::bail!("cloud session pagination returned a repeated cursor");
    }
    *cursor = Some(next_cursor);
    Ok(true)
}

fn reconcile_cloud_meeting(
    mut local: MeetingRecord,
    cloud: MeetingRecord,
    child_tombstones: &[CloudChildTombstone],
) -> Result<(MeetingRecord, bool)> {
    if local.id != cloud.id {
        anyhow::bail!(
            "cannot reconcile cloud session {} into local session {}",
            cloud.id,
            local.id
        );
    }
    if local.owner_account_id.is_some()
        && cloud.owner_account_id.is_some()
        && local.owner_account_id != cloud.owner_account_id
    {
        anyhow::bail!("cannot reconcile cloud session across account owners");
    }
    let before = serde_json::to_value(&local)?;
    let local_was_shell = !meeting_has_syncable_content(&local, None);
    if local.owner_account_id.is_none() && local_was_shell {
        local.owner_account_id = cloud.owner_account_id.clone();
    }

    apply_cloud_child_tombstones_to_meeting(&mut local, child_tombstones);

    if local_was_shell || session_title_is_placeholder(&local.title) {
        local.title = cloud.title;
    }
    if parse_ms(&local.started_at) <= 0 {
        local.started_at = cloud.started_at;
    }
    if local_was_shell && local.ended_at.is_none() {
        local.ended_at = cloud.ended_at;
    }
    if local
        .answer_instructions
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.answer_instructions = cloud.answer_instructions;
    }
    if local
        .summary
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.summary = cloud.summary;
    }

    let mut transcript_by_id = local
        .transcript
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.id, index))
        .collect::<HashMap<_, _>>();
    for cloud_segment in cloud.transcript {
        if let Some(index) = transcript_by_id.get(&cloud_segment.id).copied() {
            let segment = &mut local.transcript[index];
            if segment.text.trim().is_empty() {
                segment.text = cloud_segment.text;
            }
            if matches!(segment.speaker, Speaker::Unknown)
                && !matches!(cloud_segment.speaker, Speaker::Unknown)
            {
                segment.speaker = cloud_segment.speaker;
            }
            if parse_ms(&segment.created_at) <= 0 {
                segment.created_at = cloud_segment.created_at;
            }
            segment.is_final |= cloud_segment.is_final;
        } else {
            let index = local.transcript.len();
            transcript_by_id.insert(cloud_segment.id, index);
            local.transcript.push(cloud_segment);
        }
    }

    let mut context_by_id = local
        .context
        .iter()
        .enumerate()
        .map(|(index, artifact)| (artifact.id, index))
        .collect::<HashMap<_, _>>();
    for cloud_artifact in cloud.context {
        if let Some(index) = context_by_id.get(&cloud_artifact.id).copied() {
            merge_missing_context_fields(&mut local.context[index], cloud_artifact);
        } else {
            let index = local.context.len();
            context_by_id.insert(cloud_artifact.id, index);
            local.context.push(cloud_artifact);
        }
    }

    local
        .conversation_memory
        .merge_from(cloud.conversation_memory);
    let mut conversation_by_id = local
        .conversation
        .iter()
        .enumerate()
        .map(|(index, turn)| (turn.id, index))
        .collect::<HashMap<_, _>>();
    for cloud_turn in cloud.conversation {
        if let Some(index) = conversation_by_id.get(&cloud_turn.id).copied() {
            merge_missing_turn_fields(&mut local.conversation[index], cloud_turn);
        } else {
            let index = local.conversation.len();
            conversation_by_id.insert(cloud_turn.id, index);
            local.conversation.push(cloud_turn);
        }
    }
    local.normalize_conversation_bounds();

    merge_diagnostics(&mut local.diagnostics, cloud.diagnostics);
    if local_was_shell {
        local.live_answer_transcript_cursor = local.transcript.len();
    } else {
        local.live_answer_transcript_cursor = local
            .live_answer_transcript_cursor
            .min(local.transcript.len());
    }
    let changed = before != serde_json::to_value(&local)?;
    Ok((local, changed))
}

fn apply_cloud_child_tombstones_to_meeting(
    meeting: &mut MeetingRecord,
    child_tombstones: &[CloudChildTombstone],
) {
    let mut conversation_memory_deleted = false;
    for tombstone in child_tombstones {
        match tombstone.child_kind.as_str() {
            "transcript" => {
                let local_id = local_uuid_for_cloud_id(
                    "transcript-segment",
                    &tombstone.session_id,
                    &tombstone.child_id,
                );
                meeting.transcript.retain(|segment| segment.id != local_id);
            }
            "response" => {
                let local_id = local_turn_uuid(&tombstone.session_id, &tombstone.child_id);
                meeting.conversation.retain(|turn| turn.id != local_id);
            }
            "context" => {
                let local_id = local_uuid_for_cloud_id(
                    "context-artifact",
                    &tombstone.session_id,
                    &tombstone.child_id,
                );
                meeting.context.retain(|artifact| artifact.id != local_id);
                for turn in &mut meeting.conversation {
                    turn.attachment_ids
                        .retain(|attachment_id| *attachment_id != local_id);
                }
            }
            "rag" => {
                conversation_memory_deleted |=
                    apply_cloud_rag_tombstone_to_meeting(meeting, tombstone);
            }
            _ => {}
        }
    }
    if conversation_memory_deleted {
        // Compacted epochs have no stable server-side identity beyond their
        // positional chunk index. Clearing the derived memory projection is
        // deliberately conservative and idempotent; deleting one remote
        // memory chunk must never shift indexes and delete a different epoch
        // during a later hydration pass.
        meeting.conversation_memory = ConversationMemory::default();
    }
    meeting.normalize_conversation_bounds();
}

fn apply_cloud_rag_tombstone_to_meeting(
    meeting: &mut MeetingRecord,
    tombstone: &CloudChildTombstone,
) -> bool {
    let summary_id = format!("{}:summary:0", tombstone.session_id);
    let instructions_id = format!("{}:instructions:0", tombstone.session_id);
    if tombstone.source_kind.as_deref() == Some("summary") || tombstone.child_id == summary_id {
        meeting.summary = None;
        return false;
    }
    if tombstone.source_kind.as_deref() == Some("answer_instructions")
        || tombstone.child_id == instructions_id
    {
        meeting.answer_instructions = None;
        return false;
    }
    if tombstone.source_kind.as_deref() == Some("conversation_memory") {
        return true;
    }
    let memory_prefix = format!("{}:conversation-memory:", tombstone.session_id);
    tombstone.child_id.starts_with(&memory_prefix)
}

fn cloud_state_has_conversation_memory_tombstone(state: &CloudSyncState, session_id: &str) -> bool {
    let memory_prefix = format!("{session_id}:conversation-memory:");
    state.child_tombstones.values().any(|tombstone| {
        tombstone.child_kind == "rag"
            && (tombstone.source_kind.as_deref() == Some("conversation_memory")
                || tombstone.child_id.starts_with(&memory_prefix))
    })
}

fn merge_missing_context_fields(local: &mut ContextArtifact, cloud: ContextArtifact) {
    let local_updated_at = parse_ms(&local.updated_at);
    let cloud_updated_at = parse_ms(&cloud.updated_at);
    if cloud_updated_at > local_updated_at
        || (cloud_updated_at == local_updated_at
            && local.answer_context_role == AnswerContextRole::Other)
    {
        local.answer_context_role = cloud.answer_context_role;
    }
    if local.path.trim().is_empty() {
        local.path = cloud.path;
    }
    if local.title.trim().is_empty() {
        local.title = cloud.title;
    }
    if local
        .note
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.note = cloud.note;
    }
    if local.size_bytes.is_none() {
        local.size_bytes = cloud.size_bytes;
    }
    if local
        .text_preview
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.text_preview = cloud.text_preview;
    }
    if local
        .markdown_path
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.markdown_path = cloud.markdown_path;
    }
    if matches!(local.processing_status, ContextProcessingStatus::Pending) {
        local.processing_status = cloud.processing_status;
    }
    if local.processing_error.is_none() {
        local.processing_error = cloud.processing_error;
    }
    if parse_ms(&local.created_at) <= 0 {
        local.created_at = cloud.created_at;
    }
    if parse_ms(&cloud.updated_at) > parse_ms(&local.updated_at) {
        local.updated_at = cloud.updated_at;
    }
}

fn merge_missing_turn_fields(local: &mut ConversationTurn, cloud: ConversationTurn) {
    if local.question.trim().is_empty() {
        local.question = cloud.question;
    }
    if local.answer.trim().is_empty() {
        local.answer = cloud.answer;
    }
    let mut attachment_ids = local.attachment_ids.iter().copied().collect::<HashSet<_>>();
    for attachment_id in cloud.attachment_ids {
        if attachment_ids.insert(attachment_id) {
            local.attachment_ids.push(attachment_id);
        }
    }
    if local.artifact.is_none() {
        local.artifact = cloud.artifact;
    }
    if local
        .source
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.source = cloud.source;
    }
    if local
        .provider
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        local.provider = cloud.provider;
    }
    if parse_ms(&local.created_at) <= 0 {
        local.created_at = cloud.created_at;
    }
}

fn merge_diagnostics(local: &mut MeetingDiagnostics, cloud: MeetingDiagnostics) {
    local.listen_runs = local.listen_runs.max(cloud.listen_runs);
    local.stt_parse_errors = local.stt_parse_errors.max(cloud.stt_parse_errors);
    local.stt_provider_errors = local.stt_provider_errors.max(cloud.stt_provider_errors);
    local.audio_start_errors = local.audio_start_errors.max(cloud.audio_start_errors);
    local.audio_source_errors = local.audio_source_errors.max(cloud.audio_source_errors);
    if local.last_audio_session_id.is_none() {
        local.last_audio_session_id = cloud.last_audio_session_id;
    }
    if local.last_stt_provider.is_none() {
        local.last_stt_provider = cloud.last_stt_provider;
    }
    if local.last_error_kind.is_none() {
        local.last_error_kind = cloud.last_error_kind;
    }
    // Error strings can contain provider payloads, local paths, or user text.
    // They are intentionally device-local and never hydrated from cloud.
    if local.last_error_at.is_none() {
        local.last_error_at = cloud.last_error_at;
    }
}

fn session_title_is_placeholder(title: &str) -> bool {
    matches!(
        title.trim().to_ascii_lowercase().as_str(),
        "" | "new recording" | "untitled session" | "untitled"
    )
}

fn meeting_belongs_to_owner(meeting: &MeetingRecord, owner_account_id: Option<&str>) -> bool {
    match owner_account_id {
        Some(owner) => meeting.owner_account_id.as_deref() == Some(owner),
        None => meeting.owner_account_id.as_deref().is_none(),
    }
}

fn meeting_should_follow_cloud_delete(
    meeting: &MeetingRecord,
    owner_account_id: Option<&str>,
) -> bool {
    match owner_account_id {
        Some(owner) => meeting.owner_account_id.as_deref() == Some(owner),
        None => meeting.owner_account_id.is_none(),
    }
}

fn remove_bluey_owned_context_files(data_dir: &Path, meeting: &MeetingRecord) {
    for artifact in &meeting.context {
        remove_bluey_owned_prepared_image(data_dir, artifact);
        remove_bluey_owned_markdown(data_dir, artifact);
        if let Some(owner_account_id) = meeting.owner_account_id.as_deref() {
            remove_bluey_owned_restored_context_files(
                data_dir,
                owner_account_id,
                meeting.id,
                artifact,
            );
        }
    }
}

fn remove_cloud_tombstoned_context_files(
    data_dir: &Path,
    owner_account_id: &str,
    meeting: &MeetingRecord,
    child_tombstones: &[CloudChildTombstone],
) {
    let deleted_ids = child_tombstones
        .iter()
        .filter(|tombstone| tombstone.child_kind == "context")
        .map(|tombstone| {
            local_uuid_for_cloud_id(
                "context-artifact",
                &tombstone.session_id,
                &tombstone.child_id,
            )
        })
        .collect::<HashSet<_>>();
    for artifact in meeting
        .context
        .iter()
        .filter(|artifact| deleted_ids.contains(&artifact.id))
    {
        remove_bluey_owned_prepared_image(data_dir, artifact);
        remove_bluey_owned_markdown(data_dir, artifact);
        remove_bluey_owned_restored_context_files(data_dir, owner_account_id, meeting.id, artifact);
    }
}

fn remove_bluey_owned_restored_context_files(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
    artifact: &ContextArtifact,
) {
    let object_path = PathBuf::from(&artifact.path);
    let expected_prefix = format!("{}-", artifact.id);
    if object_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(&expected_prefix))
    {
        if let Ok(allowed_dir) = account_scoped_restored_dir(
            data_dir,
            CLOUD_RESTORED_OBJECTS_DIR,
            owner_account_id,
            session_id,
        ) {
            remove_file_under_allowed_dir(
                data_dir,
                allowed_dir,
                object_path,
                artifact.id,
                "restored object",
            );
        }
    }

    let Some(markdown_path) = artifact.markdown_path.as_deref() else {
        return;
    };
    let preview_path = PathBuf::from(markdown_path);
    let expected_preview_name = format!("{}.md", artifact.id);
    if preview_path.file_name().and_then(|name| name.to_str())
        != Some(expected_preview_name.as_str())
    {
        return;
    }
    if let Ok(allowed_dir) = account_scoped_restored_dir(
        data_dir,
        CLOUD_RESTORED_CONTEXT_DIR,
        owner_account_id,
        session_id,
    ) {
        remove_file_under_allowed_dir(
            data_dir,
            allowed_dir,
            preview_path,
            artifact.id,
            "restored preview",
        );
    }
}

fn remove_bluey_owned_prepared_image(data_dir: &Path, artifact: &ContextArtifact) {
    let path = PathBuf::from(&artifact.path);
    let expected_name = format!("{}.jpg", artifact.id);
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return;
    }
    remove_file_under_allowed_dir(
        data_dir,
        data_dir.join("context-images"),
        path,
        artifact.id,
        "prepared image",
    );
}

fn remove_bluey_owned_markdown(data_dir: &Path, artifact: &ContextArtifact) {
    let Some(markdown_path) = artifact
        .markdown_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };
    let path = PathBuf::from(markdown_path);
    let expected_name = format!("{}.md", artifact.id);
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return;
    }
    remove_file_under_allowed_dir(
        data_dir,
        data_dir.join("context-markdown"),
        path,
        artifact.id,
        "converted markdown",
    );
}

fn remove_file_under_allowed_dir(
    data_dir: &Path,
    allowed_dir: PathBuf,
    path: PathBuf,
    _artifact_id: Uuid,
    kind: &'static str,
) {
    let path = if path.is_absolute() {
        path
    } else {
        data_dir.join(path)
    };
    let allowed = match allowed_dir.canonicalize() {
        Ok(dir) => dir,
        Err(_) => allowed_dir,
    };
    let candidate = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => path,
    };
    if !candidate.starts_with(&allowed) {
        debug!("skipping cloud-delete cleanup outside the scoped Bluey cache");
        return;
    }
    if let Err(error) = fs::remove_file(&candidate) {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(
                error_kind = ?error.kind(),
                kind,
                "failed to remove a Bluey-owned context cache file after cloud delete"
            );
        }
    }
}

fn load_local_responses(
    data_dir: &Path,
    owner_account_id: &str,
    meetings: &[MeetingRecord],
) -> HashMap<String, Vec<crate::llm::CueResponse>> {
    let db_path = data_dir.join("sessions.db");
    let Ok(db) = Database::open(db_path.to_str().unwrap_or("sessions.db")) else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for meeting in meetings {
        let session_id = meeting.id.to_string();
        match db.list_all_cue_responses_for_owner(owner_account_id, &session_id) {
            Ok(responses) => {
                out.insert(session_id, responses);
            }
            Err(error) => {
                debug!(
                    session_hash = %cloud_log_identifier_hash(session_id.as_bytes()),
                    error_category = local_state_error_category(&error),
                    "cloud sync could not read local cue responses"
                );
            }
        }
    }
    out
}

fn purge_cloud_response_tombstones(
    data_dir: &Path,
    owner_account_id: &str,
    local_session_id: Uuid,
    child_tombstones: &[CloudChildTombstone],
) -> Result<()> {
    let db_path = data_dir.join("sessions.db");
    let db_path = db_path
        .to_str()
        .context("session database path is not valid UTF-8")?;
    let db = Database::open(db_path)?;
    let session_id = local_session_id.to_string();
    for tombstone in child_tombstones
        .iter()
        .filter(|tombstone| tombstone.child_kind == "response")
    {
        db.delete_cue_response_for_owner(owner_account_id, &session_id, &tombstone.child_id)?;
    }
    Ok(())
}

async fn upload_context_objects(
    data_dir: &Path,
    owner_account_id: &str,
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    sync_states: &mut HashMap<Uuid, CloudSyncState>,
    client: &CloudClient,
    operation_is_active: &OperationGuard<'_>,
) -> Result<AttachmentUploadSummary> {
    let mut summary = AttachmentUploadSummary::default();
    let mut seen = HashSet::new();
    for meeting in meetings {
        let responses = response_map.get(&meeting.id.to_string()).map(Vec::as_slice);
        for artifact in &meeting.context {
            if !seen.insert((meeting.id, artifact.id)) {
                continue;
            }
            let path = match artifact_object_source(data_dir, artifact) {
                ArtifactObjectSource::NotApplicable => continue,
                ArtifactObjectSource::Available(path) => path,
                ArtifactObjectSource::Missing => {
                    let sync_state = sync_states.get(&meeting.id).cloned();
                    let artifact_id =
                        wire_context_artifact_id(meeting, artifact.id, sync_state.as_ref());
                    record_attachment_transfer_retry(
                        data_dir,
                        owner_account_id,
                        meeting,
                        responses,
                        sync_states,
                        artifact.id,
                        artifact_id,
                        CloudAttachmentTransferStatus::UploadRetry,
                        "local_file_unavailable",
                    )?;
                    summary.retry_count += 1;
                    continue;
                }
            };
            let sync_state = sync_states.get(&meeting.id).cloned();
            let artifact_id = wire_context_artifact_id(meeting, artifact.id, sync_state.as_ref());
            let session_id = wire_session_id(meeting, sync_state.as_ref());
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(metadata) => metadata,
                Err(_) => {
                    record_attachment_transfer_retry(
                        data_dir,
                        owner_account_id,
                        meeting,
                        responses,
                        sync_states,
                        artifact.id,
                        artifact_id,
                        CloudAttachmentTransferStatus::UploadRetry,
                        "local_file_unavailable",
                    )?;
                    summary.retry_count += 1;
                    continue;
                }
            };
            if !metadata.is_file() {
                record_attachment_transfer_retry(
                    data_dir,
                    owner_account_id,
                    meeting,
                    responses,
                    sync_states,
                    artifact.id,
                    artifact_id,
                    CloudAttachmentTransferStatus::UploadRetry,
                    "local_path_not_file",
                )?;
                summary.retry_count += 1;
                continue;
            }
            if metadata.len() > MAX_OBJECT_UPLOAD_BYTES {
                record_attachment_transfer_retry(
                    data_dir,
                    owner_account_id,
                    meeting,
                    responses,
                    sync_states,
                    artifact.id,
                    artifact_id,
                    CloudAttachmentTransferStatus::UploadRetry,
                    "object_too_large",
                )?;
                summary.retry_count += 1;
                continue;
            }
            let content_type = content_type_for_path(&path);
            if Uuid::parse_str(&artifact_id).is_err() {
                // The object endpoint is UUID-keyed. Legacy non-UUID record IDs
                // still round-trip through JSON sync, but their unavailable
                // object bytes are intentionally not attached to a new ID.
                record_attachment_transfer_retry(
                    data_dir,
                    owner_account_id,
                    meeting,
                    responses,
                    sync_states,
                    artifact.id,
                    artifact_id,
                    CloudAttachmentTransferStatus::UploadRetry,
                    "unsupported_object_identifier",
                )?;
                summary.retry_count += 1;
                continue;
            }
            match tokio::fs::read(&path).await {
                Ok(bytes) => {
                    let local_sha256 = format!("{:x}", Sha256::digest(&bytes));
                    if let Some(object) = sync_states
                        .get(&meeting.id)
                        .and_then(|state| state.attachment_transfers.get(&artifact.id.to_string()))
                        .filter(|transfer| {
                            transfer.status == CloudAttachmentTransferStatus::Synced
                                && transfer.record_id == artifact_id
                        })
                        .and_then(|transfer| transfer.object.as_ref())
                        .filter(|object| {
                            object.sha256.eq_ignore_ascii_case(&local_sha256)
                                && object.size_bytes == bytes.len() as u64
                        })
                        .cloned()
                    {
                        summary.uploaded.insert(artifact.id, object);
                        continue;
                    }
                    operation_is_active()?;
                    match client
                        .upload_artifact_object(&artifact_id, &session_id, bytes, &content_type)
                        .await
                    {
                        Ok(response) => {
                            operation_is_active()?;
                            let object = SyncedObjectMetadata {
                                object_key: response.object_key,
                                size_bytes: response.size_bytes,
                                sha256: response.sha256,
                                content_type: response.content_type,
                                expires_at_ms: response.expires_at_ms,
                            };
                            if !object.sha256.eq_ignore_ascii_case(&local_sha256)
                                || object.size_bytes != metadata.len()
                            {
                                record_attachment_transfer_retry(
                                    data_dir,
                                    owner_account_id,
                                    meeting,
                                    responses,
                                    sync_states,
                                    artifact.id,
                                    artifact_id,
                                    CloudAttachmentTransferStatus::UploadRetry,
                                    "object_integrity_mismatch",
                                )?;
                                summary.retry_count += 1;
                                continue;
                            }
                            record_attachment_transfer_success(
                                data_dir,
                                owner_account_id,
                                meeting,
                                responses,
                                sync_states,
                                artifact.id,
                                artifact_id,
                                object.clone(),
                            )?;
                            summary.uploaded.insert(artifact.id, object);
                        }
                        Err(_) => {
                            record_attachment_transfer_retry(
                                data_dir,
                                owner_account_id,
                                meeting,
                                responses,
                                sync_states,
                                artifact.id,
                                artifact_id,
                                CloudAttachmentTransferStatus::UploadRetry,
                                "object_upload_failed",
                            )?;
                            summary.retry_count += 1;
                        }
                    }
                }
                Err(_) => {
                    record_attachment_transfer_retry(
                        data_dir,
                        owner_account_id,
                        meeting,
                        responses,
                        sync_states,
                        artifact.id,
                        artifact_id,
                        CloudAttachmentTransferStatus::UploadRetry,
                        "local_file_read_failed",
                    )?;
                    summary.retry_count += 1;
                }
            }
        }
    }
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn record_attachment_transfer_retry(
    data_dir: &Path,
    owner_account_id: &str,
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    sync_states: &mut HashMap<Uuid, CloudSyncState>,
    local_artifact_id: Uuid,
    record_id: String,
    status: CloudAttachmentTransferStatus,
    error_category: &str,
) -> Result<()> {
    let state = sync_states
        .entry(meeting.id)
        .or_insert_with(|| initial_cloud_sync_state(meeting, responses, owner_account_id));
    validate_cloud_sync_state_owner(state, owner_account_id)?;
    let previous_attempts = state
        .attachment_transfers
        .get(&local_artifact_id.to_string())
        .map(|transfer| transfer.attempt_count)
        .unwrap_or_default();
    state.attachment_transfers.insert(
        local_artifact_id.to_string(),
        CloudAttachmentTransferState {
            record_id,
            status,
            attempt_count: previous_attempts.saturating_add(1),
            updated_at_ms: current_epoch_ms(),
            last_error_category: Some(error_category.to_string()),
            object: None,
        },
    );
    write_cloud_sync_state(data_dir, owner_account_id, meeting.id, state)
}

#[allow(clippy::too_many_arguments)]
fn record_attachment_transfer_success(
    data_dir: &Path,
    owner_account_id: &str,
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    sync_states: &mut HashMap<Uuid, CloudSyncState>,
    local_artifact_id: Uuid,
    record_id: String,
    object: SyncedObjectMetadata,
) -> Result<()> {
    let state = sync_states
        .entry(meeting.id)
        .or_insert_with(|| initial_cloud_sync_state(meeting, responses, owner_account_id));
    validate_cloud_sync_state_owner(state, owner_account_id)?;
    let previous_attempts = state
        .attachment_transfers
        .get(&local_artifact_id.to_string())
        .map(|transfer| transfer.attempt_count)
        .unwrap_or_default();
    state.attachment_transfers.insert(
        local_artifact_id.to_string(),
        CloudAttachmentTransferState {
            record_id,
            status: CloudAttachmentTransferStatus::Synced,
            attempt_count: previous_attempts.saturating_add(1),
            updated_at_ms: current_epoch_ms(),
            last_error_category: None,
            object: Some(object),
        },
    );
    write_cloud_sync_state(data_dir, owner_account_id, meeting.id, state)
}

fn build_object_parent_batches(
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    sync_states: &HashMap<Uuid, CloudSyncState>,
) -> Vec<SyncBatchRequest> {
    let mut batches = Vec::new();
    let mut batch = SyncBatchRequest::default();
    for meeting in meetings {
        if meeting.context.is_empty() {
            continue;
        }
        if batch.sessions.len() >= MAX_SYNC_RECORDS_PER_BATCH {
            batches.push(std::mem::take(&mut batch));
        }
        let responses = response_map.get(&meeting.id.to_string()).map(Vec::as_slice);
        batch.sessions.push(session_record(
            meeting,
            responses,
            sync_states.get(&meeting.id),
        ));
    }
    if !batch.sessions.is_empty() {
        batches.push(batch);
    }
    batches
}

async fn sync_session_audit_bundles<F>(
    data_dir: &Path,
    meetings: &[MeetingRecord],
    client: &CloudClient,
    consent_is_active: F,
) -> Result<usize>
where
    F: Fn() -> bool,
{
    prune_local_audit_storage(data_dir);
    let mut uploaded = 0usize;
    for meeting in meetings {
        if !consent_is_active() {
            if let Some(owner_account_id) = meeting.owner_account_id.as_deref() {
                crate::diagnostics::fence_support_diagnostics_for_owner(
                    data_dir,
                    owner_account_id,
                )?;
            }
            break;
        }
        let scope = SessionAuditScope::from_meeting(meeting);
        if !crate::diagnostics::support_diagnostic_scope_is_uploadable(data_dir, &scope) {
            purge_session_audit_state(data_dir, meeting.owner_account_id.as_deref(), meeting.id)?;
            continue;
        }
        let session_id = meeting.id.to_string();
        let build_data_dir = data_dir.to_path_buf();
        let build_meeting = meeting.clone();
        let mut built = match tokio::task::spawn_blocking(move || {
            build_local_session_audit_bundle(&build_data_dir, &build_meeting)
        })
        .await
        {
            Ok(Ok(bundle)) => bundle,
            Ok(Err(error)) => {
                warn!(
                    session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                    error_category = local_state_error_category(&error),
                    "support diagnostic bundle build failed"
                );
                continue;
            }
            Err(_) => {
                warn!(
                    session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                    "support diagnostic bundle worker did not complete"
                );
                continue;
            }
        };
        if built.bundle.events.is_empty() {
            let _ = fs::remove_dir_all(&built.local_dir);
            continue;
        }
        if built.bytes.len() > MAX_AUDIT_BUNDLE_BYTES {
            warn!(
                session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                size_bytes = built.bytes.len(),
                max_bytes = MAX_AUDIT_BUNDLE_BYTES,
                "session audit bundle skipped because it is too large"
            );
            continue;
        }
        if audit_upload_marker_matches(data_dir, meeting, &built.bundle.bundle_id) {
            if let Err(error) = fs::remove_dir_all(&built.local_dir) {
                debug!(
                    session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                    error_kind = ?error.kind(),
                    "already uploaded session audit bundle could not be removed locally"
                );
            }
            if let Err(error) = remove_session_audit_event_log_if_unchanged(
                data_dir,
                meeting,
                built.event_log_fingerprint,
            ) {
                debug!(
                    session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                    error_category = local_state_error_category(&error),
                    "already uploaded diagnostic event log could not be removed locally"
                );
            }
            continue;
        }
        if !consent_is_active() {
            if let Some(owner_account_id) = meeting.owner_account_id.as_deref() {
                crate::diagnostics::fence_support_diagnostics_for_owner(
                    data_dir,
                    owner_account_id,
                )?;
            }
            break;
        }
        if !crate::diagnostics::support_diagnostic_scope_is_uploadable(data_dir, &scope) {
            purge_session_audit_state(data_dir, meeting.owner_account_id.as_deref(), meeting.id)?;
            continue;
        }
        match client
            .upload_session_audit_bundle(
                &session_id,
                &built.bundle.bundle_id,
                std::mem::take(&mut built.bytes),
                AUDIT_BUNDLE_CONTENT_TYPE,
            )
            .await
        {
            Ok(response) => {
                if !consent_is_active() {
                    if let Some(owner_account_id) = meeting.owner_account_id.as_deref() {
                        crate::diagnostics::fence_support_diagnostics_for_owner(
                            data_dir,
                            owner_account_id,
                        )?;
                    }
                    return Ok(uploaded);
                }
                uploaded = uploaded.saturating_add(1);
                if let Err(error) = write_audit_upload_marker(data_dir, meeting, &built, &response)
                {
                    debug!(
                        session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                        error_category = local_state_error_category(&error),
                        "session audit upload marker write failed"
                    );
                }
                if let Err(error) = fs::remove_dir_all(&built.local_dir) {
                    warn!(
                        session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                        error_kind = ?error.kind(),
                        "uploaded session audit bundle could not be removed locally"
                    );
                }
                match remove_session_audit_event_log_if_unchanged(
                    data_dir,
                    meeting,
                    built.event_log_fingerprint,
                ) {
                    Ok(true) => {}
                    Ok(false) => debug!(
                        session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                        "new diagnostic events arrived during upload; preserving them for the next bundle"
                    ),
                    Err(error) => debug!(
                        session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                        error_category = local_state_error_category(&error),
                        "uploaded diagnostic event log could not be removed locally"
                    ),
                }
            }
            Err(error) => {
                warn!(
                    session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                    error_category = support_upload_error_category(&error),
                    "support diagnostic upload deferred; local retry copy remains bounded"
                );
                return Err(anyhow::anyhow!("support diagnostic upload deferred"));
            }
        }
    }
    prune_local_audit_storage(data_dir);
    Ok(uploaded)
}

fn support_upload_error_category(error: &cue_cloud_client::Error) -> &'static str {
    match error {
        cue_cloud_client::Error::Unauthorized => "unauthorized",
        cue_cloud_client::Error::Server { status: 403 } => "forbidden",
        cue_cloud_client::Error::Server { status: 404 } => "unavailable",
        cue_cloud_client::Error::Server { status: 413 } => "too_large",
        cue_cloud_client::Error::Server { status } if *status >= 500 => "server_unavailable",
        cue_cloud_client::Error::Network(_) => "network",
        _ => "rejected",
    }
}

fn local_state_error_category(error: &anyhow::Error) -> &'static str {
    match error
        .downcast_ref::<std::io::Error>()
        .map(std::io::Error::kind)
    {
        Some(std::io::ErrorKind::NotFound) => "not_found",
        Some(std::io::ErrorKind::PermissionDenied) => "permission_denied",
        Some(std::io::ErrorKind::InvalidData) => "invalid_data",
        Some(std::io::ErrorKind::WriteZero) => "write_failed",
        Some(std::io::ErrorKind::UnexpectedEof) => "truncated",
        Some(_) => "io",
        None => "local_state",
    }
}

fn build_local_session_audit_bundle(
    data_dir: &Path,
    meeting: &MeetingRecord,
) -> Result<BuiltAuditBundle> {
    let local_dir = session_audit_bundle_dir(data_dir, meeting);
    cue_core::app_paths::create_private_dir(&local_dir)?;

    let (bundle, event_log_fingerprint) = assemble_session_audit_bundle(data_dir, meeting);
    let bytes = serde_json::to_vec_pretty(&bundle).context("serialize session audit bundle")?;
    write_json_file(&local_dir.join("manifest.json"), &bundle.manifest)?;
    write_jsonl_file(&local_dir.join("events.jsonl"), &bundle.events)?;
    write_json_file(&local_dir.join("bundle.json"), &bundle)?;

    Ok(BuiltAuditBundle {
        bundle,
        bytes,
        local_dir,
        event_log_fingerprint,
    })
}

fn assemble_session_audit_bundle(
    data_dir: &Path,
    meeting: &MeetingRecord,
) -> (SessionAuditBundle, Option<AuditEventLogFingerprint>) {
    let session_id = meeting.id.to_string();
    let session_code = short_session_code(meeting.id);
    let observed_at_ms = current_epoch_ms();
    let (raw_events, event_log_fingerprint) =
        read_session_audit_events_with_fingerprint(data_dir, meeting);
    let updated_at = event_log_fingerprint
        .map(|fingerprint| fingerprint.modified_at_ms)
        .unwrap_or(observed_at_ms);
    // Derive the bundle timestamp from the locked event-log snapshot. Retries
    // of the same fingerprint must produce identical bytes and therefore the
    // same server-side checksum/idempotency result.
    let generated_at_ms = updated_at;
    let event_log_size = event_log_fingerprint
        .map(|fingerprint| fingerprint.bytes)
        .unwrap_or(0);
    let event_log_sequence = event_log_fingerprint
        .map(|fingerprint| fingerprint.last_sequence)
        .unwrap_or(0);
    let retention_days = audit_local_retention_days();
    let retention_max_bytes = audit_local_max_bytes();
    let bundle_id = format!(
        "diagnostic-{session_code}-{updated_at}-{event_log_size}-{event_log_sequence}-{retention_days}-{retention_max_bytes}"
    );
    let events = raw_events
        .into_iter()
        .filter_map(privacy_safe_remote_audit_event)
        .collect::<Vec<_>>();

    let manifest = json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "bundle_id": bundle_id,
        "session_id": session_id,
        "session_code": session_code,
        "generated_at_ms": generated_at_ms,
        "updated_at_ms": updated_at,
        "content_policy": "metadata_only",
        "record_counts": {
            "events": events.len(),
        },
        "excluded_content": [
            "questions", "answers", "transcripts", "prompts", "audio", "screenshots",
            "files", "paths", "urls", "clipboard", "tokens", "raw_errors"
        ],
        "local_retention": {
            "uploaded_session_dirs_removed": true,
            "failed_upload_dirs_retention_days": retention_days,
            "failed_upload_root_max_bytes": retention_max_bytes,
        },
    });

    (
        SessionAuditBundle {
            schema_version: AUDIT_SCHEMA_VERSION,
            bundle_id,
            session_id,
            session_code,
            generated_at_ms,
            content_policy: "metadata_only".to_string(),
            manifest,
            events,
        },
        event_log_fingerprint,
    )
}

fn privacy_safe_remote_audit_event(value: Value) -> Option<Value> {
    let Value::Object(mut record) = value else {
        return None;
    };
    let sequence = record.remove("sequence").and_then(|value| value.as_u64())?;
    let created_at_ms = record
        .remove("created_at_ms")
        .and_then(|value| value.as_u64())?;
    let kind = record
        .remove("kind")
        .and_then(|value| {
            value
                .as_str()
                .and_then(safe_support_label)
                .map(str::to_string)
        })
        .filter(|kind| support_diagnostic_event_kind_is_known(kind))?;
    let payload = support_diagnostic_payload(record.remove("payload").unwrap_or(Value::Null));
    if payload.get("schema_version").and_then(Value::as_u64) != Some(2)
        || payload.get("event_name").and_then(Value::as_str) != Some(kind.as_str())
        || !payload
            .get("component")
            .and_then(Value::as_str)
            .is_some_and(support_diagnostic_component_is_known)
        || !payload
            .get("outcome")
            .and_then(Value::as_str)
            .is_some_and(support_diagnostic_outcome_is_known)
        || payload
            .get("created_at_ms")
            .and_then(Value::as_u64)
            .is_none()
        || payload
            .get("monotonic_offset_ms")
            .and_then(Value::as_u64)
            .is_none()
    {
        return None;
    }
    Some(json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "sequence": sequence,
        "kind": kind,
        "created_at_ms": created_at_ms,
        "source": "desktop_diagnostic",
        "content_policy": "metadata_only",
        "payload": payload,
    }))
}

fn support_diagnostic_event_kind_is_known(kind: &str) -> bool {
    matches!(
        kind,
        "overlay_user_action"
            | "overlay_lifecycle"
            | "answer_request_accepted"
            | "answer_context_prepared"
            | "answer_card_created"
            | "answer_status_presented"
            | "answer_replay_started"
            | "answer_route_completed"
            | "answer_first_text"
            | "answer_completed"
            | "answer_failed"
            | "answer_slow_start"
            | "native_first_text_rendered"
            | "native_final_rendered"
            | "transcript_settled"
            | "transcript_buffer_consumed"
            | "audio_start_requested"
            | "audio_capture_ready"
            | "audio_stop_requested"
            | "audio_capture_stopped"
            | "audio_first_chunk"
            | "stt_connected"
            | "stt_first_partial"
            | "stt_first_final"
            | "rag_query_completed"
            | "model_attempt_started"
            | "model_connected"
            | "model_first_event"
            | "model_first_text"
            | "model_attempt_completed"
            | "persistence_completed"
            | "context_watch_observed"
            | "diagnostic_events_dropped"
    )
}

fn support_diagnostic_component_is_known(component: &str) -> bool {
    matches!(
        component,
        "native_overlay" | "daemon" | "audio" | "stt" | "rag" | "model" | "persistence" | "support"
    )
}

fn support_diagnostic_outcome_is_known(outcome: &str) -> bool {
    matches!(
        outcome,
        "started" | "succeeded" | "failed" | "timed_out" | "dropped"
    )
}

fn support_diagnostic_payload(payload: Value) -> Value {
    let Value::Object(payload) = payload else {
        return json!({ "content_policy": "metadata_only" });
    };
    let mut safe = Map::new();
    for (key, value) in payload {
        let allowed = match key.as_str() {
            "schema_version"
            | "created_at_ms"
            | "monotonic_offset_ms"
            | "duration_ms"
            | "queue_wait_ms"
            | "queue_depth"
            | "queue_high_water"
            | "attempt"
            | "generation"
            | "sequence"
            | "count"
            | "bytes"
            | "input_chars"
            | "output_chars"
            | "context_count"
            | "document_count"
            | "screenshot_count"
            | "transcript_count"
            | "memory_count"
            | "source_count"
            | "dropped_count"
            | "coalesced_count" => value.is_number(),
            "streaming" => value.is_boolean(),
            "interaction_id" | "trace_id" | "request_id" | "audio_run_id" | "card_id" => value
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_some(),
            "event_name" => value
                .as_str()
                .is_some_and(support_diagnostic_event_kind_is_known),
            "component" => value
                .as_str()
                .is_some_and(support_diagnostic_component_is_known),
            "outcome" => value
                .as_str()
                .is_some_and(support_diagnostic_outcome_is_known),
            "provider" | "route" => value.as_str().is_some_and(support_provider_is_known),
            "model" => value.as_str().is_some_and(support_model_family_is_known),
            "action" => value.as_str().is_some_and(support_action_is_known),
            "error_category" => value.as_str().is_some_and(support_error_category_is_known),
            "question_intent" => value.as_str().is_some_and(support_question_intent_is_known),
            "artifact_type" => value.as_str().is_some_and(support_artifact_type_is_known),
            _ => false,
        };
        if allowed {
            safe.insert(key, value);
        }
    }
    safe.insert(
        "content_policy".to_string(),
        Value::String("metadata_only".to_string()),
    );
    Value::Object(safe)
}

fn safe_support_label(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= 48
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        }))
    .then_some(value)
}

fn support_provider_is_known(value: &str) -> bool {
    matches!(
        value,
        "anthropic"
            | "assemblyai"
            | "bluey_managed"
            | "deepgram"
            | "google"
            | "groq"
            | "local"
            | "openai"
            | "other"
    )
}

fn support_model_family_is_known(value: &str) -> bool {
    matches!(
        value,
        "bluey"
            | "claude"
            | "gemini"
            | "gpt"
            | "kimi"
            | "llama"
            | "mistral"
            | "nova"
            | "openai_reasoning"
            | "other"
            | "qwen"
            | "whisper"
    )
}

fn support_action_is_known(value: &str) -> bool {
    matches!(
        value,
        "active_page_capture_requested"
            | "analyze_screen_requested"
            | "ask_answer_sent"
            | "ask_answer_skipped"
            | "ask_requested"
            | "attach_files_requested"
            | "attach_requested"
            | "autosend_answer_sent"
            | "autosend_answer_skipped"
            | "capture_start_requested"
            | "capture_stop_requested"
            | "close_requested"
            | "context_list_requested"
            | "hidden"
            | "instructions_requested"
            | "instructions_updated"
            | "meeting_banner_action"
            | "opacity_updated"
            | "paste_text_requested"
            | "ready"
            | "recap_requested"
            | "recording_start_requested"
            | "recording_stop_requested"
            | "remove_context_requested"
            | "session_continue_requested"
            | "session_delete_requested"
            | "session_drawer_opened"
            | "session_drawer_sessions_rendered"
            | "session_list_requested"
            | "session_new_requested"
            | "session_open_requested"
            | "session_rename_requested"
            | "shortcuts_coachmark_dismissed"
            | "shortcuts_coachmark_shown"
            | "shortcuts_overlay_opened"
            | "shown"
            | "sign_in_requested"
            | "theme_changed"
            | "transcript_buffer_consumed"
            | "transcript_buffer_skip_consumed"
            | "transcript_clear_requested"
            | "transcript_context_cleared"
    )
}

fn support_error_category_is_known(value: &str) -> bool {
    matches!(
        value,
        "authentication"
            | "billing"
            | "cancelled"
            | "capacity"
            | "dropped"
            | "failed"
            | "internal"
            | "network"
            | "none"
            | "ok"
            | "rate_limit"
            | "response_db_write_failed"
            | "runtime_setup"
            | "runtime_unavailable"
            | "safety"
            | "start_canceled"
            | "timed_out"
            | "timeout"
            | "unknown"
    )
}

fn support_question_intent_is_known(value: &str) -> bool {
    matches!(
        value,
        "code_explanation"
            | "code_or_debug"
            | "explanation"
            | "general"
            | "quick_explanation"
            | "short_query"
            | "system_design"
    )
}

fn support_artifact_type_is_known(value: &str) -> bool {
    matches!(
        value,
        "code" | "document" | "none" | "screen" | "structured" | "system_design"
    )
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    serde_json::to_writer_pretty(file, value)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_jsonl_file(path: &Path, records: &[Value]) -> Result<()> {
    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    for record in records {
        serde_json::to_writer(&mut file, record)
            .with_context(|| format!("write {}", path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn session_audit_event_dir(data_dir: &Path, scope: &SessionAuditScope) -> PathBuf {
    data_dir
        .join(SESSION_AUDIT_EVENTS_DIR)
        .join(local_audit_scope_key(scope.owner_account_id.as_deref()))
        .join(scope.session_id.to_string())
}

#[cfg(test)]
pub(crate) fn session_audit_event_log_path_for_test(
    data_dir: &Path,
    scope: &SessionAuditScope,
) -> PathBuf {
    session_audit_event_dir(data_dir, scope).join("events.jsonl")
}

fn session_audit_bundle_dir(data_dir: &Path, meeting: &MeetingRecord) -> PathBuf {
    data_dir
        .join(SESSION_AUDIT_DIR)
        .join(local_audit_scope_key(meeting.owner_account_id.as_deref()))
        .join(meeting.id.to_string())
}

fn session_audit_upload_marker_path(data_dir: &Path, meeting: &MeetingRecord) -> PathBuf {
    data_dir
        .join(SESSION_AUDIT_UPLOADED_DIR)
        .join(local_audit_scope_key(meeting.owner_account_id.as_deref()))
        .join(format!("{}.json", meeting.id))
}

fn audit_event_append_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn audit_event_log_state_path(event_path: &Path) -> PathBuf {
    event_path.with_extension("state.json")
}

fn load_audit_event_log_state(event_path: &Path) -> Result<AuditEventLogState> {
    let event_bytes = fs::metadata(event_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let state_path = audit_event_log_state_path(event_path);
    if let Ok(bytes) = fs::read(&state_path) {
        if let Ok(state) = serde_json::from_slice::<AuditEventLogState>(&bytes) {
            if state.schema_version == AUDIT_EVENT_LOG_STATE_SCHEMA_VERSION
                && state.bytes == event_bytes
                && state.bytes <= MAX_AUDIT_EVENT_LOG_BYTES
                && state.record_count <= MAX_AUDIT_EVENT_RECORDS
            {
                return Ok(state);
            }
        }
    }

    // Missing or stale state means the prior process may have stopped between
    // the durable JSONL append and its small sidecar update. Stream and
    // normalize the bounded tail once; the steady-state append path remains
    // O(1).
    let state = compact_audit_event_log(event_path, 0, 0)?;
    write_audit_event_log_state(event_path, &state)?;
    Ok(state)
}

fn compact_audit_event_log(
    event_path: &Path,
    reserved_bytes: u64,
    reserved_records: u64,
) -> Result<AuditEventLogState> {
    let mut retained = VecDeque::<Vec<u8>>::new();
    let mut retained_bytes = 0_u64;
    let mut last_sequence = 0_u64;

    if let Ok(file) = fs::File::open(event_path) {
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        while let Some(truncated) = read_bounded_audit_event_line(&mut reader, &mut line)
            .with_context(|| format!("read {}", event_path.display()))?
        {
            if truncated {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if let Some(sequence) = value.get("sequence").and_then(Value::as_u64) {
                last_sequence = last_sequence.max(sequence);
            }
            let mut normalized =
                serde_json::to_vec(&value).context("serialize compacted local audit event")?;
            normalized.push(b'\n');
            let normalized_bytes = normalized.len() as u64;
            if normalized_bytes > MAX_AUDIT_EVENT_RECORD_BYTES as u64 {
                continue;
            }
            retained_bytes = retained_bytes.saturating_add(normalized_bytes);
            retained.push_back(normalized);
            while retained.len() as u64 > MAX_AUDIT_EVENT_RECORDS.saturating_sub(reserved_records)
                || retained_bytes.saturating_add(reserved_bytes) > MAX_AUDIT_EVENT_LOG_BYTES
            {
                let Some(removed) = retained.pop_front() else {
                    break;
                };
                retained_bytes = retained_bytes.saturating_sub(removed.len() as u64);
            }
        }
    }

    let mut compacted = Vec::with_capacity(retained_bytes as usize);
    for line in &retained {
        compacted.extend_from_slice(line);
    }
    atomic_write_private_file(event_path, &compacted)?;
    Ok(AuditEventLogState {
        schema_version: AUDIT_EVENT_LOG_STATE_SCHEMA_VERSION,
        last_sequence,
        record_count: retained.len() as u64,
        bytes: retained_bytes,
    })
}

fn read_bounded_audit_event_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<Option<bool>> {
    line.clear();
    let mut saw_bytes = false;
    let mut truncated = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if saw_bytes {
                Ok(Some(truncated))
            } else {
                Ok(None)
            };
        }
        saw_bytes = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let data_len = newline.unwrap_or(available.len());
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let remaining = MAX_AUDIT_EVENT_RECORD_BYTES.saturating_sub(line.len());
        let copy_len = remaining.min(data_len);
        line.extend_from_slice(&available[..copy_len]);
        if copy_len < data_len {
            truncated = true;
        }
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(Some(truncated));
        }
    }
}

fn write_audit_event_log_state(event_path: &Path, state: &AuditEventLogState) -> Result<()> {
    let bytes = serde_json::to_vec(state).context("serialize local audit event state")?;
    atomic_write_private_file(&audit_event_log_state_path(event_path), &bytes)
}

#[cfg(test)]
fn read_session_audit_events(data_dir: &Path, meeting: &MeetingRecord) -> Vec<Value> {
    read_session_audit_events_with_fingerprint(data_dir, meeting).0
}

fn read_session_audit_events_with_fingerprint(
    data_dir: &Path,
    meeting: &MeetingRecord,
) -> (Vec<Value>, Option<AuditEventLogFingerprint>) {
    let scope = SessionAuditScope::from_meeting(meeting);
    let event_path = session_audit_event_dir(data_dir, &scope).join("events.jsonl");
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !event_path.exists() {
        return (Vec::new(), None);
    }
    let Ok(state) = load_audit_event_log_state(&event_path) else {
        return (Vec::new(), None);
    };
    let Ok(file) = fs::File::open(&event_path) else {
        return (Vec::new(), None);
    };
    let events = BufReader::new(file)
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .collect();
    let fingerprint = audit_event_log_fingerprint_from_state(&event_path, &state);
    (events, fingerprint)
}

fn audit_event_log_fingerprint(
    data_dir: &Path,
    meeting: &MeetingRecord,
) -> Option<AuditEventLogFingerprint> {
    let scope = SessionAuditScope::from_meeting(meeting);
    let event_path = session_audit_event_dir(data_dir, &scope).join("events.jsonl");
    let state = load_audit_event_log_state(&event_path).ok()?;
    audit_event_log_fingerprint_from_state(&event_path, &state)
}

fn audit_event_log_fingerprint_from_state(
    event_path: &Path,
    state: &AuditEventLogState,
) -> Option<AuditEventLogFingerprint> {
    let metadata = fs::metadata(event_path).ok()?;
    Some(AuditEventLogFingerprint {
        modified_at_ms: metadata_modified_ms(&metadata),
        bytes: metadata.len(),
        last_sequence: state.last_sequence,
    })
}

#[cfg(test)]
fn remove_session_audit_event_log(data_dir: &Path, meeting: &MeetingRecord) -> Result<()> {
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    remove_session_audit_event_log_unlocked(data_dir, meeting)
}

fn remove_session_audit_event_log_if_unchanged(
    data_dir: &Path,
    meeting: &MeetingRecord,
    expected: Option<AuditEventLogFingerprint>,
) -> Result<bool> {
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current = audit_event_log_fingerprint(data_dir, meeting);
    if current != expected {
        return Ok(false);
    }
    remove_session_audit_event_log_unlocked(data_dir, meeting)?;
    Ok(true)
}

fn remove_session_audit_event_log_unlocked(data_dir: &Path, meeting: &MeetingRecord) -> Result<()> {
    let scope = SessionAuditScope::from_meeting(meeting);
    let event_dir = session_audit_event_dir(data_dir, &scope);
    if event_dir.exists() {
        fs::remove_dir_all(&event_dir)
            .with_context(|| format!("remove {}", event_dir.display()))?;
    }
    Ok(())
}

/// Remove every local diagnostic/audit artifact associated with one session.
///
/// Cloud-delete provenance is intentionally separate and remains durable
/// until the server confirms the account-side tombstone.
pub fn purge_session_audit_state(
    data_dir: &Path,
    owner_account_id: Option<&str>,
    session_id: Uuid,
) -> Result<()> {
    let scope = SessionAuditScope {
        session_id,
        owner_account_id: owner_account_id.map(ToString::to_string),
    };
    let local_bundle_dir = data_dir
        .join(SESSION_AUDIT_DIR)
        .join(local_audit_scope_key(owner_account_id))
        .join(session_id.to_string());
    let event_dir = session_audit_event_dir(data_dir, &scope);
    let upload_marker = data_dir
        .join(SESSION_AUDIT_UPLOADED_DIR)
        .join(local_audit_scope_key(owner_account_id))
        .join(format!("{session_id}.json"));

    remove_audit_path_if_present(&local_bundle_dir, true)?;
    remove_audit_path_if_present(&event_dir, true)?;
    remove_audit_path_if_present(&upload_marker, false)?;
    Ok(())
}

/// Remove all user-owned cloud-sync material for one account from this device.
///
/// The diagnostic consent epoch is advanced first and intentionally retained
/// as a payload-free fence. That prevents already-queued pre-deletion events
/// from recreating support-diagnostic content after the account purge.
pub fn purge_cloud_account_local_state(data_dir: &Path, owner_account_id: &str) -> Result<()> {
    let owner_account_id = required_owner_account_id(Some(owner_account_id))?;
    let scope_key = account_scope_key(owner_account_id)?;
    crate::diagnostics::fence_support_diagnostics_for_owner(data_dir, owner_account_id)?;

    for root in [
        CLOUD_SYNC_STATE_DIR,
        CLOUD_DELETE_OUTBOX_DIR,
        CLOUD_HYDRATION_CURSOR_DIR,
        CLOUD_RESTORED_CONTEXT_DIR,
        CLOUD_RESTORED_OBJECTS_DIR,
        SESSION_AUDIT_DIR,
        SESSION_AUDIT_EVENTS_DIR,
        SESSION_AUDIT_UPLOADED_DIR,
    ] {
        remove_audit_path_if_present(&data_dir.join(root).join(&scope_key), true)?;
    }
    Ok(())
}

/// Remove account/session-scoped sync provenance and restored attachment
/// caches after a server tombstone, even when the MeetingStore row is already
/// absent. Diagnostic/RAG/database tombstones are applied by the daemon's
/// owner-transition-fenced purge lifecycle.
pub fn purge_cloud_session_local_state(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<()> {
    let owner_account_id = required_owner_account_id(Some(owner_account_id))?;
    remove_cloud_delete_provenance(data_dir, owner_account_id, session_id)?;
    for root in [CLOUD_RESTORED_CONTEXT_DIR, CLOUD_RESTORED_OBJECTS_DIR] {
        remove_audit_path_if_present(
            &account_scoped_restored_dir(data_dir, root, owner_account_id, session_id)?,
            true,
        )?;
    }
    Ok(())
}

fn remove_audit_path_if_present(path: &Path, directory: bool) -> Result<()> {
    let result = if directory {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => sync_deleted_private_file_parent(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

fn audit_upload_marker_matches(data_dir: &Path, meeting: &MeetingRecord, bundle_id: &str) -> bool {
    let marker_path = session_audit_upload_marker_path(data_dir, meeting);
    let Ok(contents) = fs::read_to_string(marker_path) else {
        return false;
    };
    serde_json::from_str::<Value>(&contents)
        .ok()
        .and_then(|value| {
            value
                .get("bundle_id")
                .and_then(Value::as_str)
                .map(|uploaded| uploaded == bundle_id)
        })
        .unwrap_or(false)
}

fn write_audit_upload_marker(
    data_dir: &Path,
    meeting: &MeetingRecord,
    built: &BuiltAuditBundle,
    response: &SessionAuditBundleResponse,
) -> Result<()> {
    let marker_dir = data_dir
        .join(SESSION_AUDIT_UPLOADED_DIR)
        .join(local_audit_scope_key(meeting.owner_account_id.as_deref()));
    cue_core::app_paths::create_private_dir(&marker_dir)?;
    let marker_path = session_audit_upload_marker_path(data_dir, meeting);
    write_json_file(
        &marker_path,
        &json!({
            "schema_version": AUDIT_SCHEMA_VERSION,
            "session_id": meeting.id.to_string(),
            "session_code": short_session_code(meeting.id),
            "bundle_id": built.bundle.bundle_id,
            "uploaded_at_ms": current_epoch_ms(),
            "object_key": response.object_key,
            "size_bytes": response.size_bytes,
            "sha256": response.sha256,
            "expires_at_ms": response.expires_at_ms,
        }),
    )
}

fn prune_local_audit_storage(data_dir: &Path) {
    prune_local_audit_root(&data_dir.join(SESSION_AUDIT_DIR));
    prune_local_audit_root(&data_dir.join(SESSION_AUDIT_EVENTS_DIR));
}

fn prune_local_audit_root(audit_root: &Path) {
    if !audit_root.exists() {
        return;
    }
    let retention_ms = audit_local_retention_days().saturating_mul(86_400_000);
    let cutoff_ms = current_epoch_ms().saturating_sub(retention_ms);
    let mut entries = audit_session_dirs(audit_root);
    for entry in &entries {
        if entry.modified_ms <= cutoff_ms {
            if let Err(error) = fs::remove_dir_all(&entry.path) {
                warn!(
                    error_kind = ?error.kind(),
                    "failed to prune expired local session audit directory"
                );
            }
        }
    }

    entries = audit_session_dirs(audit_root);
    let max_bytes = audit_local_max_bytes();
    let mut total: u64 = entries.iter().map(|entry| entry.size_bytes).sum();
    if total <= max_bytes {
        return;
    }
    entries.sort_by_key(|entry| entry.modified_ms);
    for entry in entries {
        if total <= max_bytes {
            break;
        }
        if let Err(error) = fs::remove_dir_all(&entry.path) {
            warn!(
                error_kind = ?error.kind(),
                "failed to prune local session audit directory for size cap"
            );
            continue;
        }
        total = total.saturating_sub(entry.size_bytes);
    }
}

#[derive(Debug, Clone)]
struct AuditDirEntry {
    path: PathBuf,
    modified_ms: i64,
    size_bytes: u64,
}

fn audit_session_dirs(root: &Path) -> Vec<AuditDirEntry> {
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if Uuid::parse_str(name).is_ok() {
            if let Some(entry) = audit_dir_entry(path) {
                sessions.push(entry);
            }
            continue;
        }
        let Ok(scoped_entries) = fs::read_dir(&path) else {
            continue;
        };
        sessions.extend(scoped_entries.flatten().filter_map(|entry| {
            let session_path = entry.path();
            let session_name = session_path.file_name()?.to_str()?;
            if !session_path.is_dir() || Uuid::parse_str(session_name).is_err() {
                return None;
            }
            audit_dir_entry(session_path)
        }));
    }
    sessions
}

fn audit_dir_entry(path: PathBuf) -> Option<AuditDirEntry> {
    let metadata = fs::metadata(&path).ok()?;
    Some(AuditDirEntry {
        modified_ms: metadata_modified_ms(&metadata),
        size_bytes: dir_size_bytes(&path),
        path,
    })
}

fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(read_dir) = fs::read_dir(path) else {
        return 0;
    };
    read_dir
        .filter_map(|entry| entry.ok())
        .map(|entry| dir_size_bytes(&entry.path()))
        .sum()
}

fn metadata_modified_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn audit_local_retention_days() -> i64 {
    std::env::var("BLUEY_AUDIT_LOCAL_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_AUDIT_LOCAL_RETENTION_DAYS)
        .clamp(1, 30)
}

fn audit_local_max_bytes() -> u64 {
    std::env::var("BLUEY_AUDIT_LOCAL_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_AUDIT_LOCAL_MAX_BYTES)
}

fn current_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn cloud_log_identifier_hash(identifier: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-cloud-log-identifier-v1\0");
    hasher.update((identifier.len() as u64).to_be_bytes());
    hasher.update(identifier);
    format!("{:x}", hasher.finalize())
        .chars()
        .take(12)
        .collect()
}

fn empty_metadata() -> Value {
    json!({})
}

fn stable_entity_uuid(entity: &str, session_id: &str, source_id: &str) -> Uuid {
    let mut hasher = Sha256::new();
    for part in ["bluey-cloud-sync-v1", entity, session_id, source_id] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // UUIDv8 reserves this layout for application-defined deterministic IDs.
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn required_owner_account_id(owner_account_id: Option<&str>) -> Result<&str> {
    owner_account_id
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
        .ok_or_else(|| anyhow::anyhow!("cloud sync requires an account owner"))
}

fn account_scope_key(owner_account_id: &str) -> Result<String> {
    let owner_account_id = required_owner_account_id(Some(owner_account_id))?;
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-cloud-account-scope-v1\0");
    hasher.update((owner_account_id.len() as u64).to_be_bytes());
    hasher.update(owner_account_id.as_bytes());
    Ok(format!("account-{:x}", hasher.finalize()))
}

fn local_audit_scope_key(owner_account_id: Option<&str>) -> String {
    owner_account_id
        .and_then(|owner| account_scope_key(owner).ok())
        .unwrap_or_else(|| "local-unowned".to_string())
}

fn local_uuid_for_cloud_id(entity: &str, session_id: &str, source_id: &str) -> Uuid {
    Uuid::parse_str(source_id).unwrap_or_else(|_| stable_entity_uuid(entity, session_id, source_id))
}

fn local_turn_uuid(session_id: &str, response_id: &str) -> Uuid {
    response_id
        .strip_prefix("turn-")
        .and_then(|value| Uuid::parse_str(value).ok())
        .or_else(|| Uuid::parse_str(response_id).ok())
        .unwrap_or_else(|| stable_entity_uuid("turn", session_id, response_id))
}

fn stable_wire_record_id(entity: &str, session_id: &str, source_id: &str) -> String {
    let trimmed = source_id.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 192
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        trimmed.to_string()
    } else {
        stable_entity_uuid(entity, session_id, source_id).to_string()
    }
}

fn cloud_sync_state_dir(data_dir: &Path, owner_account_id: &str) -> Result<PathBuf> {
    Ok(data_dir
        .join(CLOUD_SYNC_STATE_DIR)
        .join(account_scope_key(owner_account_id)?))
}

fn cloud_sync_state_path(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<PathBuf> {
    Ok(cloud_sync_state_dir(data_dir, owner_account_id)?.join(format!("{session_id}.json")))
}

fn cloud_sync_state_backup_path(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<PathBuf> {
    Ok(cloud_sync_state_dir(data_dir, owner_account_id)?.join(format!("{session_id}.json.bak")))
}

fn cloud_delete_outbox_dir(data_dir: &Path, owner_account_id: &str) -> Result<PathBuf> {
    Ok(data_dir
        .join(CLOUD_DELETE_OUTBOX_DIR)
        .join(account_scope_key(owner_account_id)?))
}

fn cloud_delete_outbox_path(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<PathBuf> {
    Ok(cloud_delete_outbox_dir(data_dir, owner_account_id)?.join(format!("{session_id}.json")))
}

fn cloud_hydration_cursor_path(data_dir: &Path, owner_account_id: &str) -> Result<PathBuf> {
    Ok(data_dir
        .join(CLOUD_HYDRATION_CURSOR_DIR)
        .join(account_scope_key(owner_account_id)?)
        .join("cursor.json"))
}

fn load_cloud_hydration_cursor(data_dir: &Path, owner_account_id: &str) -> Option<String> {
    let path = cloud_hydration_cursor_path(data_dir, owner_account_id).ok()?;
    let state = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CloudHydrationCursorState>(&bytes).ok())?;
    (state.schema_version == CLOUD_HYDRATION_CURSOR_SCHEMA_VERSION
        && state.owner_account_id == owner_account_id
        && !state.cursor.trim().is_empty())
    .then_some(state.cursor)
}

fn write_cloud_hydration_cursor(
    data_dir: &Path,
    owner_account_id: &str,
    cursor: &str,
) -> Result<()> {
    let cursor = cursor.trim();
    anyhow::ensure!(!cursor.is_empty(), "cloud hydration cursor is empty");
    let state = CloudHydrationCursorState {
        schema_version: CLOUD_HYDRATION_CURSOR_SCHEMA_VERSION,
        owner_account_id: owner_account_id.to_string(),
        cursor: cursor.to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&state).context("serialize cloud hydration cursor")?;
    let path = cloud_hydration_cursor_path(data_dir, owner_account_id)?;
    let parent = path
        .parent()
        .context("cloud hydration cursor has no parent")?;
    cue_core::app_paths::create_private_dir(parent)?;
    atomic_write_private_file(&path, &bytes)
}

fn clear_cloud_hydration_cursor(data_dir: &Path, owner_account_id: &str) -> Result<()> {
    remove_audit_path_if_present(
        &cloud_hydration_cursor_path(data_dir, owner_account_id)?,
        false,
    )
}

fn write_pending_cloud_delete(data_dir: &Path, pending: &PendingCloudSessionDelete) -> Result<()> {
    let dir = cloud_delete_outbox_dir(data_dir, &pending.owner_account_id)?;
    cue_core::app_paths::create_private_dir(&dir)?;
    let bytes =
        serde_json::to_vec_pretty(pending).context("serialize pending cloud session deletion")?;
    atomic_write_private_file(
        &cloud_delete_outbox_path(
            data_dir,
            &pending.owner_account_id,
            pending.local_session_id,
        )?,
        &bytes,
    )
}

fn remove_cloud_delete_provenance(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<()> {
    for path in [
        cloud_delete_outbox_path(data_dir, owner_account_id, session_id)?,
        cloud_sync_state_path(data_dir, owner_account_id, session_id)?,
        cloud_sync_state_backup_path(data_dir, owner_account_id, session_id)?,
    ] {
        remove_audit_path_if_present(&path, false)?;
    }
    Ok(())
}

fn load_cloud_sync_state(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Option<CloudSyncState> {
    let path = cloud_sync_state_path(data_dir, owner_account_id, session_id).ok()?;
    if let Some(state) = read_valid_cloud_sync_state(&path, owner_account_id) {
        return Some(state);
    }

    let backup_path = cloud_sync_state_backup_path(data_dir, owner_account_id, session_id).ok()?;
    let recovered = read_valid_cloud_sync_state(&backup_path, owner_account_id);
    if recovered.is_some() {
        warn!(
            session_hash = %cloud_log_identifier_hash(session_id.as_bytes()),
            "recovered cloud sync deletion provenance from the last valid backup"
        );
    }
    recovered
}

fn load_cloud_sync_state_strict_if_present(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<Option<CloudSyncState>> {
    let path = cloud_sync_state_path(data_dir, owner_account_id, session_id)?;
    let backup_path = cloud_sync_state_backup_path(data_dir, owner_account_id, session_id)?;
    let primary_exists = path.exists();
    let backup_exists = backup_path.exists();
    if !primary_exists && !backup_exists {
        return Ok(None);
    }
    if let Some(state) = read_valid_cloud_sync_state(&path, owner_account_id)
        .or_else(|| read_valid_cloud_sync_state(&backup_path, owner_account_id))
    {
        return Ok(Some(state));
    }
    anyhow::bail!("cloud deletion provenance is corrupt")
}

fn read_valid_cloud_sync_state(path: &Path, owner_account_id: &str) -> Option<CloudSyncState> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice::<CloudSyncState>(&bytes) {
        Ok(state) if validate_cloud_sync_state_owner(&state, owner_account_id).is_ok() => {
            Some(state)
        }
        Ok(_) => None,
        Err(_) => {
            debug!("cloud sync state could not be parsed");
            None
        }
    }
}

fn validate_cloud_sync_state_owner(state: &CloudSyncState, owner_account_id: &str) -> Result<()> {
    let owner_account_id = required_owner_account_id(Some(owner_account_id))?;
    if state.schema_version != CLOUD_SYNC_STATE_SCHEMA_VERSION {
        anyhow::bail!("unsupported cloud sync state schema");
    }
    if state.owner_account_id != owner_account_id {
        anyhow::bail!("cloud sync state account owner mismatch");
    }
    if state.remote_session_id.trim().is_empty() {
        anyhow::bail!("cloud sync state remote session id is empty");
    }
    Ok(())
}

fn cloud_session_fingerprint(summary: &CloudSessionSummary) -> CloudSessionFingerprint {
    CloudSessionFingerprint {
        title: summary.title.clone(),
        status: summary.status.clone(),
        updated_at_ms: summary.updated_at_ms,
        last_active_at_ms: summary.last_active_at_ms,
        answer_style: summary.answer_style.clone(),
        transcript_count: summary.transcript_count,
        response_count: summary.response_count,
        context_count: summary.context_count,
        rag_count: summary.rag_count,
        child_tombstone_count: summary.child_tombstone_count,
        child_tombstone_updated_at_ms: summary.child_tombstone_updated_at_ms,
    }
}

fn cloud_session_summary_is_current(
    data_dir: &Path,
    owner_account_id: &str,
    local_session_id: Uuid,
    summary: &CloudSessionSummary,
) -> bool {
    summary.child_tombstone_count == 0
        && load_cloud_sync_state(data_dir, owner_account_id, local_session_id).is_some_and(
            |state| {
                state.remote_session_id == summary.session_id
                    && state.remote_summary.as_ref() == Some(&cloud_session_fingerprint(summary))
                    && state
                        .attachment_transfers
                        .values()
                        .all(|transfer| transfer.status == CloudAttachmentTransferStatus::Synced)
            },
        )
}

fn mark_cloud_session_summary_current(
    data_dir: &Path,
    owner_account_id: &str,
    local_session_id: Uuid,
    summary: &CloudSessionSummary,
) -> Result<()> {
    let mut state = load_cloud_sync_state(data_dir, owner_account_id, local_session_id)
        .context("cloud bundle sync state missing after hydration")?;
    anyhow::ensure!(
        state.remote_session_id == summary.session_id,
        "cloud hydration summary session identity mismatch"
    );
    state.remote_summary = Some(cloud_session_fingerprint(summary));
    write_cloud_sync_state(data_dir, owner_account_id, local_session_id, &state)
}

fn load_cloud_sync_states(
    data_dir: &Path,
    owner_account_id: &str,
    meetings: &[MeetingRecord],
) -> HashMap<Uuid, CloudSyncState> {
    meetings
        .iter()
        .filter_map(|meeting| {
            load_cloud_sync_state(data_dir, owner_account_id, meeting.id)
                .map(|state| (meeting.id, state))
        })
        .collect()
}

fn write_cloud_sync_state(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
    state: &CloudSyncState,
) -> Result<()> {
    validate_cloud_sync_state_owner(state, owner_account_id)?;
    let dir = cloud_sync_state_dir(data_dir, owner_account_id)?;
    cue_core::app_paths::create_private_dir(&dir)?;
    let path = cloud_sync_state_path(data_dir, owner_account_id, session_id)?;
    let backup_path = cloud_sync_state_backup_path(data_dir, owner_account_id, session_id)?;
    if read_valid_cloud_sync_state(&path, owner_account_id).is_some() {
        let current = fs::read(&path)
            .with_context(|| format!("read current cloud sync state {}", path.display()))?;
        atomic_write_private_file(&backup_path, &current)?;
    }
    let bytes =
        serde_json::to_vec_pretty(state).context("serialize cloud sync deletion provenance")?;
    atomic_write_private_file(&path, &bytes)
}

fn atomic_write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".tmp-{}-{}", std::process::id(), Uuid::new_v4()));
    let temporary = PathBuf::from(temporary);
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("set permissions on {}", temporary.display()))?;
        }
        file.write_all(bytes)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync {}", temporary.display()))?;
        drop(file);
        atomic_replace_sync_state_file(&temporary, path).with_context(|| {
            format!(
                "replace cloud sync state {} with {}",
                path.display(),
                temporary.display()
            )
        })?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .with_context(|| format!("sync directory {}", parent.display()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_deleted_private_file_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("sync directory {}", parent.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(unix)]
fn atomic_replace_sync_state_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn atomic_replace_sync_state_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace_sync_state_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

fn cloud_sync_state_after_upload(
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
    previous: Option<&CloudSyncState>,
    uploaded_children: Option<&UploadedChildState>,
    owner_account_id: &str,
) -> CloudSyncState {
    let session = session_record(meeting, responses, previous);
    let context_artifacts = meeting
        .context
        .iter()
        .map(|artifact| {
            let record = context_record(meeting, artifact, uploaded_objects, previous);
            (
                artifact.id.to_string(),
                CloudContextState {
                    record_id: record.artifact_id,
                    source_uri: record.source_uri,
                    content_hash: record.content_hash,
                    updated_at_ms: record.updated_at_ms,
                    metadata: record.metadata,
                },
            )
        })
        .collect();
    let current_context_ids = meeting
        .context
        .iter()
        .map(|artifact| artifact.id.to_string())
        .collect::<HashSet<_>>();
    let mut attachment_transfers = previous
        .map(|state| state.attachment_transfers.clone())
        .unwrap_or_default();
    attachment_transfers.retain(|local_id, _| current_context_ids.contains(local_id));
    CloudSyncState {
        schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
        owner_account_id: owner_account_id.to_string(),
        remote_session_id: session.session_id,
        remote_summary: None,
        session_metadata: session.metadata,
        transcript_segments: previous
            .map(|state| state.transcript_segments.clone())
            .unwrap_or_default(),
        context_artifacts,
        responses: previous
            .map(|state| state.responses.clone())
            .unwrap_or_default(),
        synced_transcript_records: uploaded_children
            .map(|children| children.transcript_records.clone())
            .unwrap_or_default(),
        synced_response_records: uploaded_children
            .map(|children| children.response_records.clone())
            .unwrap_or_default(),
        rag_chunks: uploaded_children
            .map(|children| children.rag_chunks.clone())
            .unwrap_or_default(),
        attachment_transfers,
        child_tombstones: previous
            .map(|state| state.child_tombstones.clone())
            .unwrap_or_default(),
    }
}

fn initial_cloud_sync_state(
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    owner_account_id: &str,
) -> CloudSyncState {
    let session = session_record(meeting, responses, None);
    CloudSyncState {
        schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
        owner_account_id: owner_account_id.to_string(),
        remote_session_id: session.session_id,
        remote_summary: None,
        session_metadata: session.metadata,
        transcript_segments: BTreeMap::new(),
        context_artifacts: BTreeMap::new(),
        responses: BTreeMap::new(),
        synced_transcript_records: BTreeMap::new(),
        synced_response_records: BTreeMap::new(),
        rag_chunks: BTreeMap::new(),
        attachment_transfers: BTreeMap::new(),
        child_tombstones: BTreeMap::new(),
    }
}

fn uploaded_child_states_by_session(
    batches: &[SyncBatchRequest],
) -> HashMap<String, UploadedChildState> {
    let mut by_session = HashMap::<String, UploadedChildState>::new();
    for batch in batches {
        for record in &batch.transcript_segments {
            if record.deleted_at_ms.is_some() {
                continue;
            }
            by_session
                .entry(record.session_id.clone())
                .or_default()
                .transcript_records
                .insert(
                    record.segment_id.clone(),
                    CloudTranscriptState {
                        record_id: record.segment_id.clone(),
                        speaker: record.speaker.clone(),
                        source: record.source.clone(),
                        start_ms: record.start_ms,
                        end_ms: record.end_ms,
                        ts_ms: record.ts_ms,
                        metadata: record.metadata.clone(),
                    },
                );
        }
        for record in &batch.cue_responses {
            if record.deleted_at_ms.is_some() {
                continue;
            }
            by_session
                .entry(record.session_id.clone())
                .or_default()
                .response_records
                .insert(
                    record.response_id.clone(),
                    CloudResponseState {
                        record_id: record.response_id.clone(),
                        kind: record.kind.clone(),
                        ts_ms: record.ts_ms,
                        model: record.model.clone(),
                        lane: record.lane.clone(),
                        task_type: record.task_type.clone(),
                        cost_cents: record.cost_cents,
                        balance_cents_after: record.balance_cents_after,
                        cost_label: record.cost_label.clone(),
                        metadata: record.metadata.clone(),
                    },
                );
        }
        for record in &batch.rag_chunks {
            if record.deleted_at_ms.is_some() {
                continue;
            }
            let Some(session_id) = record.session_id.as_ref() else {
                continue;
            };
            by_session
                .entry(session_id.clone())
                .or_default()
                .rag_chunks
                .insert(
                    record.chunk_id.clone(),
                    CloudRagState {
                        session_id: record.session_id.clone(),
                        source_kind: record.source_kind.clone(),
                        source_id: record.source_id.clone(),
                        chunk_index: record.chunk_index,
                        updated_at_ms: record.updated_at_ms,
                    },
                );
        }
    }
    by_session
}

fn restored_rag_states(
    meeting: &MeetingRecord,
    state: &CloudSyncState,
) -> BTreeMap<String, CloudRagState> {
    let session_id = wire_session_id(meeting, Some(state));
    let mut records = Vec::new();
    for segment in &meeting.transcript {
        if segment.is_final {
            if let Some(record) = transcript_rag_chunk(meeting, segment, Some(state)) {
                records.push(record);
            }
        }
    }
    for artifact in &meeting.context {
        if let Some(record) = context_rag_chunk(meeting, artifact, Some(state)) {
            records.push(record);
        }
    }
    if let Some(summary) = meeting.summary.as_deref().and_then(truncate_nonempty) {
        records.push(SyncRagChunkRecord {
            chunk_id: format!("{session_id}:summary:0"),
            session_id: Some(session_id.clone()),
            source_kind: "summary".into(),
            source_id: session_id.clone(),
            chunk_index: 0,
            text: summary,
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms: updated_at_ms(meeting, None),
            deleted_at_ms: None,
            metadata: json!({}),
        });
    }
    for (index, epoch) in meeting.conversation_memory.epochs.iter().enumerate() {
        let Some(text) = truncate_nonempty(&epoch.summary) else {
            continue;
        };
        records.push(SyncRagChunkRecord {
            chunk_id: format!("{session_id}:conversation-memory:{index}"),
            session_id: Some(session_id.clone()),
            source_kind: "conversation_memory".into(),
            source_id: session_id.clone(),
            chunk_index: index as i64,
            text,
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms: parse_ms(&epoch.last_created_at),
            deleted_at_ms: None,
            metadata: json!({}),
        });
    }
    if let Some(instructions) = meeting
        .answer_instructions
        .as_deref()
        .and_then(truncate_nonempty)
    {
        records.push(SyncRagChunkRecord {
            chunk_id: format!("{session_id}:instructions:0"),
            session_id: Some(session_id.clone()),
            source_kind: "answer_instructions".into(),
            source_id: session_id,
            chunk_index: 0,
            text: instructions,
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms: updated_at_ms(meeting, None),
            deleted_at_ms: None,
            metadata: json!({}),
        });
    }
    for turn in &meeting.conversation {
        if let Some(record) = conversation_rag_chunk(meeting, turn, Some(state)) {
            records.push(record);
        }
    }
    records
        .into_iter()
        .map(|record| {
            let chunk_id = record.chunk_id.clone();
            (
                chunk_id,
                CloudRagState {
                    session_id: record.session_id,
                    source_kind: record.source_kind,
                    source_id: record.source_id,
                    chunk_index: record.chunk_index,
                    updated_at_ms: record.updated_at_ms,
                },
            )
        })
        .collect()
}

fn wire_session_id(meeting: &MeetingRecord, state: Option<&CloudSyncState>) -> String {
    state
        .map(|state| state.remote_session_id.trim())
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| meeting.id.to_string())
}

fn wire_context_artifact_id(
    meeting: &MeetingRecord,
    artifact_id: Uuid,
    state: Option<&CloudSyncState>,
) -> String {
    let session_id = wire_session_id(meeting, state);
    state
        .and_then(|state| state.context_artifacts.get(&artifact_id.to_string()))
        .map(|record| stable_wire_record_id("context-artifact", &session_id, &record.record_id))
        .unwrap_or_else(|| artifact_id.to_string())
}

fn wire_transcript_segment_id(
    meeting: &MeetingRecord,
    segment_id: Uuid,
    state: Option<&CloudSyncState>,
) -> String {
    let session_id = wire_session_id(meeting, state);
    state
        .and_then(|state| state.transcript_segments.get(&segment_id.to_string()))
        .map(|record| stable_wire_record_id("transcript-segment", &session_id, &record.record_id))
        .unwrap_or_else(|| segment_id.to_string())
}

fn merge_metadata(mut preserved: Value, current: Value) -> Value {
    let Value::Object(current) = current else {
        return preserved;
    };
    if !preserved.is_object() {
        preserved = json!({});
    }
    let target = preserved.as_object_mut().expect("object initialized above");
    target.extend(current);
    preserved
}

fn artifact_object_source(data_dir: &Path, artifact: &ContextArtifact) -> ArtifactObjectSource {
    let restored_preview_dir = data_dir.join(CLOUD_RESTORED_CONTEXT_DIR);
    let restored_object_dir = data_dir.join(CLOUD_RESTORED_OBJECTS_DIR);
    let candidates = std::iter::once(Some(artifact.path.as_str()))
        .chain(std::iter::once(artifact.markdown_path.as_deref()));
    let mut saw_local_candidate = false;
    for candidate in candidates.flatten() {
        if candidate.trim().is_empty() {
            continue;
        }
        if candidate.starts_with("http://")
            || candidate.starts_with("https://")
            || candidate.starts_with("bluey://")
        {
            continue;
        }
        let path = PathBuf::from(candidate);
        if path.starts_with(&restored_preview_dir) || path.starts_with(&restored_object_dir) {
            continue;
        }
        saw_local_candidate = true;
        if path.is_file() {
            return ArtifactObjectSource::Available(path);
        }
    }
    if saw_local_candidate {
        ArtifactObjectSource::Missing
    } else {
        ArtifactObjectSource::NotApplicable
    }
}

fn content_type_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
fn build_sync_batches(
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
) -> Vec<SyncBatchRequest> {
    build_sync_batches_with_states(meetings, response_map, uploaded_objects, &HashMap::new())
}

fn build_sync_batches_with_states(
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
    sync_states: &HashMap<Uuid, CloudSyncState>,
) -> Vec<SyncBatchRequest> {
    let mut batches = Vec::new();

    for meeting in meetings {
        let local_session_id = meeting.id.to_string();
        let responses = response_map.get(&local_session_id);
        let sync_state = sync_states.get(&meeting.id);
        let has_pending_child_deletions = sync_state.is_some_and(|state| {
            !state.context_artifacts.is_empty()
                || !state.synced_transcript_records.is_empty()
                || !state.synced_response_records.is_empty()
                || !state.rag_chunks.is_empty()
        });
        if !meeting_has_syncable_content(meeting, responses) && !has_pending_child_deletions {
            debug!(
                session_hash = %cloud_log_identifier_hash(meeting.id.as_bytes()),
                "cloud sync skipped empty local meeting shell"
            );
            continue;
        }

        let session_id = wire_session_id(meeting, sync_state);
        let session = session_record(meeting, responses.map(Vec::as_slice), sync_state);
        let mut batch = SyncBatchRequest::default();
        batch.sessions.push(session.clone());

        let mut seen_response_ids = HashSet::new();

        for segment in &meeting.transcript {
            let transcript_record = transcript_record(meeting, segment, sync_state);
            if sync_state.is_some_and(|state| {
                cloud_child_tombstone_dominates(
                    &state.child_tombstones,
                    "transcript",
                    &transcript_record.segment_id,
                    transcript_record.ts_ms,
                )
            }) {
                continue;
            }
            maybe_flush(&mut batches, &mut batch, &session);
            batch.transcript_segments.push(transcript_record);

            if segment.is_final {
                if let Some(chunk) = transcript_rag_chunk(meeting, segment, sync_state) {
                    if sync_state.is_some_and(|state| {
                        cloud_child_tombstone_dominates(
                            &state.child_tombstones,
                            "rag",
                            &chunk.chunk_id,
                            chunk.updated_at_ms,
                        )
                    }) {
                        continue;
                    }
                    maybe_flush(&mut batches, &mut batch, &session);
                    batch.rag_chunks.push(chunk);
                }
            }
        }

        for artifact in &meeting.context {
            let context_record = context_record(meeting, artifact, uploaded_objects, sync_state);
            if sync_state.is_some_and(|state| {
                cloud_child_tombstone_dominates(
                    &state.child_tombstones,
                    "context",
                    &context_record.artifact_id,
                    context_record
                        .updated_at_ms
                        .max(context_record.created_at_ms),
                )
            }) {
                continue;
            }
            maybe_flush(&mut batches, &mut batch, &session);
            batch.context_artifacts.push(context_record);
            if let Some(chunk) = context_rag_chunk(meeting, artifact, sync_state) {
                if sync_state.is_some_and(|state| {
                    cloud_child_tombstone_dominates(
                        &state.child_tombstones,
                        "rag",
                        &chunk.chunk_id,
                        chunk.updated_at_ms,
                    )
                }) {
                    continue;
                }
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(chunk);
            }
        }

        if let Some(sync_state) = sync_state {
            let current_context_ids = meeting
                .context
                .iter()
                .map(|artifact| artifact.id.to_string())
                .collect::<HashSet<_>>();
            for (local_id, preserved) in &sync_state.context_artifacts {
                if current_context_ids.contains(local_id) {
                    continue;
                }
                let artifact_id =
                    stable_wire_record_id("context-artifact", &session_id, &preserved.record_id);
                let deleted_at_ms = chrono::Utc::now()
                    .timestamp_millis()
                    .max(preserved.updated_at_ms.saturating_add(1));
                maybe_flush(&mut batches, &mut batch, &session);
                batch.context_artifacts.push(SyncContextArtifactRecord {
                    artifact_id: artifact_id.clone(),
                    session_id: session_id.clone(),
                    kind: "deleted".into(),
                    title: "Deleted context".into(),
                    note: None,
                    source_uri: None,
                    content_hash: preserved.content_hash.clone(),
                    text_preview: None,
                    created_at_ms: 0,
                    updated_at_ms: deleted_at_ms,
                    deleted_at_ms: Some(deleted_at_ms),
                    metadata: json!({ "tombstone": true }),
                });
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(SyncRagChunkRecord {
                    chunk_id: format!("{session_id}:context:{artifact_id}:0"),
                    session_id: Some(session_id.clone()),
                    source_kind: "context".into(),
                    source_id: artifact_id,
                    chunk_index: 0,
                    text: String::new(),
                    embedding: None,
                    embedding_model: None,
                    token_count: None,
                    content_hash: None,
                    updated_at_ms: deleted_at_ms,
                    deleted_at_ms: Some(deleted_at_ms),
                    metadata: json!({ "tombstone": true }),
                });
            }
        }

        if let Some(summary) = meeting.summary.as_deref().and_then(truncate_nonempty) {
            let record = SyncRagChunkRecord {
                chunk_id: format!("{session_id}:summary:0"),
                session_id: Some(session_id.clone()),
                source_kind: "summary".into(),
                source_id: session_id.clone(),
                chunk_index: 0,
                text: summary,
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: updated_at_ms(meeting, responses.map(Vec::as_slice)),
                deleted_at_ms: None,
                metadata: json!({}),
            };
            if !sync_state.is_some_and(|state| {
                cloud_child_tombstone_dominates(
                    &state.child_tombstones,
                    "rag",
                    &record.chunk_id,
                    record.updated_at_ms,
                )
            }) {
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(record);
            }
        }

        if !sync_state
            .is_some_and(|state| cloud_state_has_conversation_memory_tombstone(state, &session_id))
        {
            for (index, epoch) in meeting.conversation_memory.epochs.iter().enumerate() {
                let Some(text) = truncate_nonempty(&epoch.summary) else {
                    continue;
                };
                let record = SyncRagChunkRecord {
                    chunk_id: format!("{session_id}:conversation-memory:{index}"),
                    session_id: Some(session_id.clone()),
                    source_kind: "conversation_memory".into(),
                    source_id: session_id.clone(),
                    chunk_index: index as i64,
                    text,
                    embedding: None,
                    embedding_model: None,
                    token_count: None,
                    content_hash: None,
                    updated_at_ms: parse_ms(&epoch.last_created_at),
                    deleted_at_ms: None,
                    metadata: json!({
                        "memory_revision": meeting.conversation_memory.revision,
                        "epoch_revision": epoch.revision,
                        "turn_count": epoch.turn_count,
                        "first_turn_id": epoch.first_turn_id,
                        "last_turn_id": epoch.last_turn_id,
                        "derived": true,
                    }),
                };
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(record);
            }
        }

        if let Some(instructions) = meeting
            .answer_instructions
            .as_deref()
            .and_then(truncate_nonempty)
        {
            let record = SyncRagChunkRecord {
                chunk_id: format!("{session_id}:instructions:0"),
                session_id: Some(session_id.clone()),
                source_kind: "answer_instructions".into(),
                source_id: session_id.clone(),
                chunk_index: 0,
                text: instructions,
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: updated_at_ms(meeting, responses.map(Vec::as_slice)),
                deleted_at_ms: None,
                metadata: json!({}),
            };
            if !sync_state.is_some_and(|state| {
                cloud_child_tombstone_dominates(
                    &state.child_tombstones,
                    "rag",
                    &record.chunk_id,
                    record.updated_at_ms,
                )
            }) {
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(record);
            }
        }

        if let Some(responses) = responses {
            for response in responses {
                let response_record = cue_response_record(meeting, response, sync_state);
                let response_id = response_record.response_id.clone();
                if sync_state.is_some_and(|state| {
                    cloud_child_tombstone_dominates(
                        &state.child_tombstones,
                        "response",
                        &response_id,
                        response_record.ts_ms,
                    )
                }) {
                    continue;
                }
                seen_response_ids.insert(response_id.clone());
                maybe_flush(&mut batches, &mut batch, &session);
                batch.cue_responses.push(response_record);
                if let Some(chunk) = response_rag_chunk(meeting, response, &response_id, sync_state)
                {
                    if sync_state.is_some_and(|state| {
                        cloud_child_tombstone_dominates(
                            &state.child_tombstones,
                            "rag",
                            &chunk.chunk_id,
                            chunk.updated_at_ms,
                        )
                    }) {
                        continue;
                    }
                    maybe_flush(&mut batches, &mut batch, &session);
                    batch.rag_chunks.push(chunk);
                }
            }
        }

        for turn in &meeting.conversation {
            let response_record = conversation_response_record(meeting, turn, sync_state);
            if seen_response_ids.contains(&response_record.response_id) {
                continue;
            }
            if sync_state.is_some_and(|state| {
                cloud_child_tombstone_dominates(
                    &state.child_tombstones,
                    "response",
                    &response_record.response_id,
                    response_record.ts_ms,
                )
            }) {
                continue;
            }
            maybe_flush(&mut batches, &mut batch, &session);
            batch.cue_responses.push(response_record);
            if let Some(chunk) = conversation_rag_chunk(meeting, turn, sync_state) {
                if sync_state.is_some_and(|state| {
                    cloud_child_tombstone_dominates(
                        &state.child_tombstones,
                        "rag",
                        &chunk.chunk_id,
                        chunk.updated_at_ms,
                    )
                }) {
                    continue;
                }
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(chunk);
            }
        }

        if let Some(sync_state) = sync_state {
            let all_batches = batches.iter().chain(std::iter::once(&batch));
            let current_transcript_ids = all_batches
                .clone()
                .flat_map(|batch| &batch.transcript_segments)
                .filter(|record| record.session_id == session_id && record.deleted_at_ms.is_none())
                .map(|record| record.segment_id.clone())
                .collect::<HashSet<_>>();
            let current_response_ids = all_batches
                .clone()
                .flat_map(|batch| &batch.cue_responses)
                .filter(|record| record.session_id == session_id && record.deleted_at_ms.is_none())
                .map(|record| record.response_id.clone())
                .collect::<HashSet<_>>();
            let current_rag_ids = all_batches
                .clone()
                .flat_map(|batch| &batch.rag_chunks)
                .filter(|record| {
                    record.session_id.as_deref() == Some(session_id.as_str())
                        && record.deleted_at_ms.is_none()
                })
                .map(|record| record.chunk_id.clone())
                .collect::<HashSet<_>>();
            let already_tombstoned_rag_ids = all_batches
                .flat_map(|batch| &batch.rag_chunks)
                .filter(|record| {
                    record.session_id.as_deref() == Some(session_id.as_str())
                        && record.deleted_at_ms.is_some()
                })
                .map(|record| record.chunk_id.clone())
                .collect::<HashSet<_>>();
            let deleted_now_ms = chrono::Utc::now().timestamp_millis();

            for (record_id, previous) in &sync_state.synced_transcript_records {
                if current_transcript_ids.contains(record_id) {
                    continue;
                }
                let deleted_at_ms = deleted_now_ms.max(previous.ts_ms.saturating_add(1));
                maybe_flush(&mut batches, &mut batch, &session);
                batch.transcript_segments.push(SyncTranscriptSegment {
                    segment_id: record_id.clone(),
                    session_id: session_id.clone(),
                    speaker: previous.speaker.clone(),
                    source: previous.source.clone(),
                    text: String::new(),
                    start_ms: previous.start_ms,
                    end_ms: previous.end_ms,
                    ts_ms: deleted_at_ms,
                    is_final: true,
                    deleted_at_ms: Some(deleted_at_ms),
                    metadata: json!({ "tombstone": true }),
                });
            }

            for (record_id, previous) in &sync_state.synced_response_records {
                if current_response_ids.contains(record_id) {
                    continue;
                }
                let deleted_at_ms = deleted_now_ms.max(previous.ts_ms.saturating_add(1));
                maybe_flush(&mut batches, &mut batch, &session);
                batch.cue_responses.push(SyncCueResponseRecord {
                    response_id: record_id.clone(),
                    session_id: session_id.clone(),
                    kind: previous.kind.clone(),
                    text: String::new(),
                    source_text: None,
                    ts_ms: deleted_at_ms,
                    provider: None,
                    model: previous.model.clone(),
                    lane: previous.lane.clone(),
                    task_type: previous.task_type.clone(),
                    cost_cents: None,
                    balance_cents_after: None,
                    cost_label: None,
                    artifact_type: None,
                    artifact_body: None,
                    artifact_confidence: None,
                    deleted_at_ms: Some(deleted_at_ms),
                    metadata: json!({ "tombstone": true }),
                });
            }

            for (chunk_id, previous) in &sync_state.rag_chunks {
                if current_rag_ids.contains(chunk_id)
                    || already_tombstoned_rag_ids.contains(chunk_id)
                {
                    continue;
                }
                let deleted_at_ms = deleted_now_ms.max(previous.updated_at_ms.saturating_add(1));
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(SyncRagChunkRecord {
                    chunk_id: chunk_id.clone(),
                    session_id: previous
                        .session_id
                        .clone()
                        .or_else(|| Some(session_id.clone())),
                    source_kind: previous.source_kind.clone(),
                    source_id: previous.source_id.clone(),
                    chunk_index: previous.chunk_index,
                    text: String::new(),
                    embedding: None,
                    embedding_model: None,
                    token_count: None,
                    content_hash: None,
                    updated_at_ms: deleted_at_ms,
                    deleted_at_ms: Some(deleted_at_ms),
                    metadata: json!({ "tombstone": true }),
                });
            }
        }

        if batch_total(&batch) > 0 {
            batches.push(batch);
        }
    }

    batches
}

fn meeting_has_syncable_content(
    meeting: &MeetingRecord,
    responses: Option<&Vec<crate::llm::CueResponse>>,
) -> bool {
    !meeting.transcript.is_empty()
        || !meeting.context.is_empty()
        || !meeting.conversation.is_empty()
        || !meeting.conversation_memory.is_empty()
        || responses.is_some_and(|items| !items.is_empty())
        || meeting
            .summary
            .as_deref()
            .is_some_and(|summary| !summary.trim().is_empty())
}

async fn meeting_from_cloud_bundle(
    data_dir: &Path,
    client: Option<&CloudClient>,
    owner_account_id: &str,
    bundle: CloudSessionBundle,
    operation_is_active: &OperationGuard<'_>,
) -> Result<(MeetingRecord, usize, Vec<CloudChildTombstone>)> {
    operation_is_active()?;
    validate_cloud_bundle_parentage(&bundle)?;
    let CloudSessionBundle {
        session,
        mut transcript_segments,
        mut cue_responses,
        mut context_artifacts,
        mut rag_chunks,
        child_tombstones,
    } = bundle;
    let remote_session_id = session.session_id.clone();
    let id = local_uuid_for_cloud_id("session", "account-session", &remote_session_id);
    let previous_state = load_cloud_sync_state(data_dir, owner_account_id, id);
    let tombstones = newest_cloud_child_tombstones(
        child_tombstones
            .into_iter()
            .chain(
                previous_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.child_tombstones.values().cloned()),
            )
            .collect(),
    );
    transcript_segments.retain(|record| {
        !cloud_child_tombstone_dominates(
            &tombstones,
            "transcript",
            &record.segment_id,
            record.ts_ms,
        )
    });
    cue_responses.retain(|record| {
        !cloud_child_tombstone_dominates(&tombstones, "response", &record.response_id, record.ts_ms)
    });
    context_artifacts.retain(|record| {
        !cloud_child_tombstone_dominates(
            &tombstones,
            "context",
            &record.artifact_id,
            record.updated_at_ms.max(record.created_at_ms),
        )
    });
    rag_chunks.retain(|record| {
        !cloud_child_tombstone_dominates(&tombstones, "rag", &record.chunk_id, record.updated_at_ms)
    });
    let mut sync_state = CloudSyncState {
        schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
        owner_account_id: owner_account_id.to_string(),
        remote_session_id: remote_session_id.clone(),
        remote_summary: None,
        session_metadata: session.metadata.clone(),
        transcript_segments: BTreeMap::new(),
        context_artifacts: BTreeMap::new(),
        responses: BTreeMap::new(),
        synced_transcript_records: BTreeMap::new(),
        synced_response_records: BTreeMap::new(),
        rag_chunks: BTreeMap::new(),
        attachment_transfers: previous_state
            .as_ref()
            .map(|state| state.attachment_transfers.clone())
            .unwrap_or_default(),
        child_tombstones: tombstones.clone(),
    };

    let mut transcript = Vec::new();
    for segment in transcript_segments {
        let local_id = local_uuid_for_cloud_id(
            "transcript-segment",
            &remote_session_id,
            &segment.segment_id,
        );
        // MeetingRecord intentionally has no STT source/start/end fields. Keep
        // those exact cloud values in the sync sidecar so a hydrate-upload
        // cycle does not replace them with speaker-derived guesses.
        let transcript_state = CloudTranscriptState {
            record_id: segment.segment_id.clone(),
            speaker: segment.speaker.clone(),
            source: segment.source.clone(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            ts_ms: segment.ts_ms,
            metadata: segment.metadata.clone(),
        };
        sync_state
            .transcript_segments
            .insert(local_id.to_string(), transcript_state.clone());
        sync_state
            .synced_transcript_records
            .insert(segment.segment_id.clone(), transcript_state);
        transcript.push(TranscriptSegment {
            id: local_id,
            speaker: speaker_from_cloud(&segment.speaker, &segment.source),
            text: segment.text,
            created_at: segment.ts_ms.to_string(),
            is_final: segment.is_final,
        });
    }

    let mut context = Vec::new();
    let mut attachment_retry_count = 0usize;
    for artifact in context_artifacts {
        operation_is_active()?;
        let local_id = local_uuid_for_cloud_id(
            "context-artifact",
            &remote_session_id,
            &artifact.artifact_id,
        );
        sync_state.context_artifacts.insert(
            local_id.to_string(),
            CloudContextState {
                record_id: artifact.artifact_id.clone(),
                source_uri: artifact.source_uri.clone(),
                content_hash: artifact.content_hash.clone(),
                updated_at_ms: artifact.updated_at_ms.max(artifact.created_at_ms),
                metadata: artifact.metadata.clone(),
            },
        );
        let (artifact, transfer) = context_artifact_from_cloud(
            RestoredContextScope {
                data_dir,
                owner_account_id,
                session_id: id,
            },
            client,
            local_id,
            artifact,
            sync_state.attachment_transfers.get(&local_id.to_string()),
            operation_is_active,
        )
        .await?;
        if let Some(transfer) = transfer {
            if transfer.status != CloudAttachmentTransferStatus::Synced {
                attachment_retry_count = attachment_retry_count.saturating_add(1);
            }
            sync_state
                .attachment_transfers
                .insert(local_id.to_string(), transfer);
            write_cloud_sync_state(data_dir, owner_account_id, id, &sync_state)?;
        } else {
            sync_state
                .attachment_transfers
                .remove(&local_id.to_string());
        }
        context.push(artifact);
    }

    let mut conversation = Vec::new();
    for response in cue_responses {
        let local_id = local_turn_uuid(&remote_session_id, &response.response_id);
        if let Some(turn) = conversation_turn_from_cloud(response.clone(), local_id) {
            let response_state = CloudResponseState {
                record_id: response.response_id,
                kind: response.kind,
                ts_ms: response.ts_ms,
                model: response.model,
                lane: response.lane,
                task_type: response.task_type,
                cost_cents: response.cost_cents,
                balance_cents_after: response.balance_cents_after,
                cost_label: response.cost_label,
                metadata: response.metadata,
            };
            sync_state
                .responses
                .insert(local_id.to_string(), response_state.clone());
            sync_state
                .synced_response_records
                .insert(response_state.record_id.clone(), response_state);
            conversation.push(turn);
        }
    }

    let live_answer_transcript_cursor = transcript.len();
    let conversation_memory = conversation_memory_from_session_metadata(&session.metadata);
    let mut meeting = MeetingRecord {
        id,
        owner_account_id: None,
        title: session.title,
        started_at: session.created_at_ms.to_string(),
        ended_at: if session.status == "active" {
            None
        } else {
            Some(session.updated_at_ms.to_string())
        },
        transcript,
        // A restored cloud transcript is historical context. Only speech
        // captured after resume should be submitted by the live Answer action.
        live_answer_transcript_cursor,
        action_items: Vec::new(),
        decisions: Vec::new(),
        context,
        conversation,
        conversation_memory,
        answer_instructions: session.answer_style,
        diagnostics: cloud_diagnostics_from_metadata(&session.metadata),
        summary: session
            .metadata
            .get("summary")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
    };
    let tombstone_values = tombstones.values().cloned().collect::<Vec<_>>();
    apply_cloud_child_tombstones_to_meeting(&mut meeting, &tombstone_values);
    let live_context_ids = meeting
        .context
        .iter()
        .map(|artifact| artifact.id.to_string())
        .collect::<HashSet<_>>();
    sync_state
        .attachment_transfers
        .retain(|local_id, _| live_context_ids.contains(local_id));
    sync_state.rag_chunks = if rag_chunks.is_empty() {
        restored_rag_states(&meeting, &sync_state)
    } else {
        rag_chunks
            .into_iter()
            .map(|record| {
                let chunk_id = record.chunk_id.clone();
                (
                    chunk_id,
                    CloudRagState {
                        session_id: record.session_id,
                        source_kind: record.source_kind,
                        source_id: record.source_id,
                        chunk_index: record.chunk_index,
                        updated_at_ms: record.updated_at_ms,
                    },
                )
            })
            .collect()
    };
    operation_is_active()?;
    write_cloud_sync_state(data_dir, owner_account_id, id, &sync_state)?;
    Ok((meeting, attachment_retry_count, tombstone_values))
}

fn validate_cloud_bundle_parentage(bundle: &CloudSessionBundle) -> Result<()> {
    let session_id = bundle.session.session_id.as_str();
    for child_session_id in bundle
        .transcript_segments
        .iter()
        .map(|record| record.session_id.as_str())
        .chain(
            bundle
                .cue_responses
                .iter()
                .map(|record| record.session_id.as_str()),
        )
        .chain(
            bundle
                .context_artifacts
                .iter()
                .map(|record| record.session_id.as_str()),
        )
        .chain(
            bundle
                .rag_chunks
                .iter()
                .filter_map(|record| record.session_id.as_deref()),
        )
        .chain(
            bundle
                .child_tombstones
                .iter()
                .map(|record| record.session_id.as_str()),
        )
    {
        if child_session_id != session_id {
            anyhow::bail!(
                "cloud session bundle child parent mismatch: expected {session_id}, got {child_session_id}"
            );
        }
    }

    for tombstone in &bundle.child_tombstones {
        if tombstone.child_id.trim().is_empty()
            || tombstone.deleted_at_ms <= 0
            || !matches!(
                tombstone.child_kind.as_str(),
                "transcript" | "response" | "context" | "rag"
            )
        {
            anyhow::bail!("cloud session bundle contained an invalid child tombstone");
        }
    }

    let artifact_ids = bundle
        .context_artifacts
        .iter()
        .map(|record| record.artifact_id.as_str())
        .chain(
            bundle
                .child_tombstones
                .iter()
                .filter(|tombstone| tombstone.child_kind == "context")
                .map(|tombstone| tombstone.child_id.as_str()),
        )
        .collect::<HashSet<_>>();
    for response in &bundle.cue_responses {
        let Some(value) = response.metadata.get("attachment_ids") else {
            continue;
        };
        let Some(values) = value.as_array() else {
            if value.is_null() {
                continue;
            }
            anyhow::bail!(
                "cloud response {} has invalid attachment metadata",
                response.response_id
            );
        };
        for value in values {
            let Some(artifact_id) = value.as_str() else {
                anyhow::bail!(
                    "cloud response {} has a non-string attachment id",
                    response.response_id
                );
            };
            if !artifact_ids.contains(artifact_id) {
                anyhow::bail!(
                    "cloud response {} references attachment {} outside its parent session",
                    response.response_id,
                    artifact_id
                );
            }
        }
    }
    for tombstone in bundle
        .child_tombstones
        .iter()
        .filter(|tombstone| tombstone.child_kind == "rag")
    {
        let provenance_fields = [
            tombstone.source_kind.is_some(),
            tombstone.source_id.is_some(),
            tombstone.chunk_index.is_some(),
        ];
        let present = provenance_fields
            .into_iter()
            .filter(|present| *present)
            .count();
        if present != 0 && present != provenance_fields.len() {
            anyhow::bail!("cloud session bundle contained partial RAG tombstone provenance");
        }
    }
    Ok(())
}

fn cloud_child_tombstone_key(child_kind: &str, child_id: &str) -> String {
    format!("{}:{child_id}", child_kind.trim().to_ascii_lowercase())
}

fn newest_cloud_child_tombstones(
    child_tombstones: Vec<CloudChildTombstone>,
) -> BTreeMap<String, CloudChildTombstone> {
    let mut newest = BTreeMap::<String, CloudChildTombstone>::new();
    for tombstone in child_tombstones {
        let key = cloud_child_tombstone_key(&tombstone.child_kind, &tombstone.child_id);
        let replace = newest
            .get(&key)
            .is_none_or(|current| tombstone.deleted_at_ms > current.deleted_at_ms);
        if replace {
            newest.insert(key, tombstone);
        }
    }
    newest
}

fn cloud_child_tombstone_dominates(
    tombstones: &BTreeMap<String, CloudChildTombstone>,
    child_kind: &str,
    child_id: &str,
    updated_at_ms: i64,
) -> bool {
    tombstones
        .get(&cloud_child_tombstone_key(child_kind, child_id))
        .is_some_and(|tombstone| tombstone.deleted_at_ms >= updated_at_ms)
}

#[derive(Clone, Copy)]
struct RestoredContextScope<'a> {
    data_dir: &'a Path,
    owner_account_id: &'a str,
    session_id: Uuid,
}

async fn context_artifact_from_cloud(
    scope: RestoredContextScope<'_>,
    client: Option<&CloudClient>,
    id: Uuid,
    record: SyncContextArtifactRecord,
    previous_transfer: Option<&CloudAttachmentTransferState>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<(ContextArtifact, Option<CloudAttachmentTransferState>)> {
    let kind = context_kind_from_cloud(&record.kind);
    let status = processing_status_from_metadata(&record.metadata, record.text_preview.as_deref());
    operation_is_active()?;
    let expected_preview_path = account_scoped_restored_dir(
        scope.data_dir,
        CLOUD_RESTORED_CONTEXT_DIR,
        scope.owner_account_id,
        scope.session_id,
    )?
    .join(format!("{id}.md"));
    let preview_result = write_restored_context_preview(
        scope.data_dir,
        scope.owner_account_id,
        scope.session_id,
        id,
        &record,
    );
    let (preview_path, preview_retry) = match preview_result {
        Ok(path) => (path, None),
        Err(_) => (
            expected_preview_path,
            Some(attachment_transfer_retry_state(
                record.artifact_id.clone(),
                CloudAttachmentTransferStatus::DownloadRetry,
                "preview_write_failed",
                previous_transfer,
            )),
        ),
    };
    let (object_path, transfer) = match (preview_retry, client) {
        (Some(retry), _) => (preview_path.clone(), Some(retry)),
        (None, Some(client)) => match download_restored_context_object(
            scope,
            client,
            id,
            &record,
            previous_transfer,
            operation_is_active,
        )
        .await?
        {
            Some((path, transfer)) => (path, Some(transfer)),
            None => (preview_path.clone(), None),
        },
        (None, None) => (preview_path.clone(), None),
    };
    let size_bytes = record
        .metadata
        .get("object_size_bytes")
        .or_else(|| record.metadata.get("size_bytes"))
        .and_then(|value| value.as_u64());
    Ok((
        ContextArtifact {
            id,
            kind,
            path: object_path.display().to_string(),
            title: record.title,
            // `source_uri` and object restoration status are transport metadata,
            // not user-authored notes. The sidecar preserves the former exactly.
            note: record.note,
            size_bytes,
            text_preview: record.text_preview,
            markdown_path: Some(preview_path.display().to_string()),
            processing_status: status,
            processing_error: record
                .metadata
                .get("processing_error")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            answer_context_role: answer_context_role_from_metadata(&record.metadata),
            created_at: record.created_at_ms.to_string(),
            updated_at: record.updated_at_ms.max(record.created_at_ms).to_string(),
        },
        transfer,
    ))
}

fn write_restored_context_preview(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
    id: Uuid,
    record: &SyncContextArtifactRecord,
) -> Result<std::path::PathBuf> {
    let dir = account_scoped_restored_dir(
        data_dir,
        CLOUD_RESTORED_CONTEXT_DIR,
        owner_account_id,
        session_id,
    )?;
    cue_core::app_paths::create_private_dir(&dir)?;
    let path = dir.join(format!("{id}.md"));
    let mut body = format!(
        "# {}\n\nKind: {}\nRestored from Bluey Cloud.\n",
        record.title, record.kind
    );
    if let Some(note) = record
        .note
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        body.push_str("\n## Note\n\n");
        body.push_str(note.trim());
        body.push('\n');
    }
    if let Some(preview) = record
        .text_preview
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        body.push_str("\n## Preview\n\n");
        body.push_str(preview.trim());
        body.push('\n');
    } else {
        body.push_str("\nNo text preview was synced for this item.\n");
    }
    crate::storage::write_private_atomic_bytes(&path, body.as_bytes())
        .context("write account-scoped restored context preview")?;
    Ok(path)
}

async fn download_restored_context_object(
    scope: RestoredContextScope<'_>,
    client: &CloudClient,
    id: Uuid,
    record: &SyncContextArtifactRecord,
    previous_transfer: Option<&CloudAttachmentTransferState>,
    operation_is_active: &OperationGuard<'_>,
) -> Result<Option<(PathBuf, CloudAttachmentTransferState)>> {
    let object_key = record
        .metadata
        .get("object_key")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty());
    if object_key.is_none() {
        return Ok(None);
    }
    operation_is_active()?;
    let bytes = match client.download_artifact_object(&record.artifact_id).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = error;
            return Ok(Some((
                account_scoped_restored_dir(
                    scope.data_dir,
                    CLOUD_RESTORED_CONTEXT_DIR,
                    scope.owner_account_id,
                    scope.session_id,
                )?
                .join(format!("{id}.md")),
                attachment_transfer_retry_state(
                    record.artifact_id.clone(),
                    CloudAttachmentTransferStatus::DownloadRetry,
                    "object_download_failed",
                    previous_transfer,
                ),
            )));
        }
    };
    operation_is_active()?;
    let preview_path = account_scoped_restored_dir(
        scope.data_dir,
        CLOUD_RESTORED_CONTEXT_DIR,
        scope.owner_account_id,
        scope.session_id,
    )?
    .join(format!("{id}.md"));
    let local_sha256 = format!("{:x}", Sha256::digest(&bytes));
    let expected_size = record
        .metadata
        .get("object_size_bytes")
        .and_then(Value::as_u64);
    let expected_sha256 = record
        .metadata
        .get("object_sha256")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if bytes.len() as u64 > MAX_OBJECT_UPLOAD_BYTES
        || expected_size.is_some_and(|expected| expected != bytes.len() as u64)
        || expected_sha256.is_some_and(|expected| !expected.eq_ignore_ascii_case(&local_sha256))
    {
        return Ok(Some((
            preview_path,
            attachment_transfer_retry_state(
                record.artifact_id.clone(),
                CloudAttachmentTransferStatus::DownloadRetry,
                "object_integrity_mismatch",
                previous_transfer,
            ),
        )));
    }
    let dir = account_scoped_restored_dir(
        scope.data_dir,
        CLOUD_RESTORED_OBJECTS_DIR,
        scope.owner_account_id,
        scope.session_id,
    )?;
    if cue_core::app_paths::create_private_dir(&dir).is_err() {
        return Ok(Some((
            preview_path,
            attachment_transfer_retry_state(
                record.artifact_id.clone(),
                CloudAttachmentTransferStatus::DownloadRetry,
                "object_directory_unavailable",
                previous_transfer,
            ),
        )));
    }
    let filename = restored_object_filename(id, record);
    let path = dir.join(filename);
    if crate::storage::write_private_atomic_bytes(&path, &bytes).is_err() {
        return Ok(Some((
            preview_path,
            attachment_transfer_retry_state(
                record.artifact_id.clone(),
                CloudAttachmentTransferStatus::DownloadRetry,
                "object_write_failed",
                previous_transfer,
            ),
        )));
    }
    let object = SyncedObjectMetadata {
        object_key: object_key.unwrap_or_default().to_string(),
        size_bytes: bytes.len() as u64,
        sha256: local_sha256,
        content_type: record
            .metadata
            .get("object_content_type")
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string(),
        expires_at_ms: record
            .metadata
            .get("object_expires_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    };
    Ok(Some((
        path,
        CloudAttachmentTransferState {
            record_id: record.artifact_id.clone(),
            status: CloudAttachmentTransferStatus::Synced,
            attempt_count: previous_transfer
                .map(|transfer| transfer.attempt_count)
                .unwrap_or_default()
                .saturating_add(1),
            updated_at_ms: current_epoch_ms(),
            last_error_category: None,
            object: Some(object),
        },
    )))
}

fn account_scoped_restored_dir(
    data_dir: &Path,
    root: &str,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<PathBuf> {
    Ok(data_dir
        .join(root)
        .join(account_scope_key(owner_account_id)?)
        .join(session_id.to_string()))
}

fn attachment_transfer_retry_state(
    record_id: String,
    status: CloudAttachmentTransferStatus,
    error_category: &str,
    previous_transfer: Option<&CloudAttachmentTransferState>,
) -> CloudAttachmentTransferState {
    CloudAttachmentTransferState {
        record_id,
        status,
        attempt_count: previous_transfer
            .map(|transfer| transfer.attempt_count)
            .unwrap_or_default()
            .saturating_add(1),
        updated_at_ms: current_epoch_ms(),
        last_error_category: Some(error_category.to_string()),
        object: None,
    }
}

fn restored_object_filename(id: Uuid, record: &SyncContextArtifactRecord) -> String {
    let mut title = sanitize_filename(&record.title);
    if title.is_empty() {
        title = "artifact".to_string();
    }
    if Path::new(&title).extension().is_none() {
        if let Some(ext) = extension_for_content_type(
            record
                .metadata
                .get("object_content_type")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        ) {
            title.push('.');
            title.push_str(ext);
        }
    }
    format!("{id}-{title}")
}

fn sanitize_filename(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().take(96) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches(|ch| ch == ' ' || ch == '.').to_string()
}

fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next().unwrap_or_default().trim() {
        "application/pdf" => Some("pdf"),
        "application/msword" => Some("doc"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.ms-excel" => Some("xls"),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.ms-powerpoint" => Some("ppt"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        "text/csv" => Some("csv"),
        "text/plain" => Some("txt"),
        "text/markdown" => Some("md"),
        "application/json" => Some("json"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/heic" => Some("heic"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

fn conversation_turn_from_cloud(
    response: SyncCueResponseRecord,
    id: Uuid,
) -> Option<ConversationTurn> {
    if response.text.trim().is_empty() {
        return None;
    }
    // Some response kinds have no source question. An empty local question is
    // the faithful representation; synthesizing one would alter user history.
    let question = response.source_text.unwrap_or_default();
    let attachment_ids = attachment_ids_from_metadata(&response.metadata, &response.session_id);
    let artifact = cloud_response_artifact(
        response.artifact_type,
        response.artifact_body,
        response.artifact_confidence,
        &response.metadata,
    );
    Some(ConversationTurn {
        id,
        question,
        answer: response.text,
        attachment_ids,
        artifact,
        source: response
            .metadata
            .get("source")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        provider: response.provider,
        created_at: response.ts_ms.to_string(),
    })
}

fn cloud_response_artifact(
    artifact_type: Option<String>,
    artifact_body: Option<String>,
    artifact_confidence: Option<f32>,
    metadata: &Value,
) -> Option<CueCardArtifact> {
    let body = artifact_body?.trim().to_string();
    if body.is_empty() {
        return None;
    }
    let artifact_type = match artifact_type?.trim().to_ascii_lowercase().as_str() {
        "code" | "patch" | "diff" => CardArtifactType::Code,
        "system_design" | "system-design" | "architecture" | "design" => {
            CardArtifactType::SystemDesign
        }
        "screen" => CardArtifactType::Screen,
        "document" => CardArtifactType::Document,
        "structured" => CardArtifactType::Structured,
        _ => return None,
    };
    let fallback_title = match artifact_type {
        CardArtifactType::Code => "Code canvas",
        CardArtifactType::SystemDesign => "System design canvas",
        CardArtifactType::Screen => "Screen context",
        CardArtifactType::Document => "Document context",
        CardArtifactType::Structured => "Details",
    };
    // Legacy response rows did not carry a canvas title. New uploads preserve
    // it in metadata; the fallback is only a type label, not reconstructed
    // customer content.
    let title = metadata
        .get("canvas_artifact_title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_title);
    Some(CueCardArtifact {
        artifact_type,
        title: title.to_string(),
        body,
        confidence: artifact_confidence.unwrap_or(0.88).clamp(0.0, 1.0),
    })
}

fn attachment_ids_from_metadata(metadata: &Value, session_id: &str) -> Vec<Uuid> {
    metadata
        .get("attachment_ids")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(|value| local_uuid_for_cloud_id("context-artifact", session_id, value))
        .collect()
}

fn speaker_from_cloud(speaker: &str, source: &str) -> Speaker {
    match (
        speaker.trim().to_ascii_lowercase().as_str(),
        source.trim().to_ascii_lowercase().as_str(),
    ) {
        ("system", _) | (_, "system") => Speaker::System,
        ("user", _) | (_, "microphone") | (_, "mic") => Speaker::User,
        ("other", _) => Speaker::Other,
        _ => Speaker::Unknown,
    }
}

fn context_kind_from_cloud(kind: &str) -> ContextKind {
    match kind.trim().to_ascii_lowercase().as_str() {
        "image" => ContextKind::Image,
        "diagram" => ContextKind::Diagram,
        "code" => ContextKind::Code,
        "document" => ContextKind::Document,
        "text" => ContextKind::Text,
        _ => ContextKind::Other,
    }
}

fn processing_status_from_metadata(
    metadata: &serde_json::Value,
    text_preview: Option<&str>,
) -> ContextProcessingStatus {
    match metadata
        .get("processing_status")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "ready" => ContextProcessingStatus::Ready,
        "unsupported" => ContextProcessingStatus::Unsupported,
        "failed" => ContextProcessingStatus::Failed,
        _ if text_preview.is_some_and(|value| !value.trim().is_empty()) => {
            ContextProcessingStatus::Ready
        }
        _ => ContextProcessingStatus::Pending,
    }
}

fn answer_context_role_from_metadata(metadata: &serde_json::Value) -> AnswerContextRole {
    metadata
        .get("answer_context_role")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn maybe_flush(
    batches: &mut Vec<SyncBatchRequest>,
    batch: &mut SyncBatchRequest,
    session: &SyncSessionRecord,
) {
    if batch_total(batch) < MAX_SYNC_RECORDS_PER_BATCH {
        return;
    }
    let mut next = SyncBatchRequest::default();
    next.sessions.push(session.clone());
    let full = std::mem::replace(batch, next);
    if batch_total(&full) > 0 {
        batches.push(full);
    }
}

fn batch_total(batch: &SyncBatchRequest) -> usize {
    batch.sessions.len()
        + batch.transcript_segments.len()
        + batch.cue_responses.len()
        + batch.context_artifacts.len()
        + batch.rag_chunks.len()
}

fn session_record(
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    state: Option<&CloudSyncState>,
) -> SyncSessionRecord {
    let updated_at_ms = updated_at_ms(meeting, responses);
    let session_id = wire_session_id(meeting, state);
    let summary_chunk_id = format!("{session_id}:summary:0");
    let instructions_chunk_id = format!("{session_id}:instructions:0");
    let summary = state
        .filter(|state| {
            state
                .child_tombstones
                .contains_key(&cloud_child_tombstone_key("rag", &summary_chunk_id))
        })
        .map(|_| None)
        .unwrap_or(meeting.summary.as_deref());
    let answer_instructions = state
        .filter(|state| {
            state
                .child_tombstones
                .contains_key(&cloud_child_tombstone_key("rag", &instructions_chunk_id))
        })
        .map(|_| None)
        .unwrap_or(meeting.answer_instructions.as_deref());
    let mut conversation_memory = meeting.conversation_memory.clone();
    if state.is_some_and(|state| cloud_state_has_conversation_memory_tombstone(state, &session_id))
    {
        conversation_memory = ConversationMemory::default();
    }
    let metadata = merge_metadata(
        state
            .map(|state| state.session_metadata.clone())
            .unwrap_or_else(empty_metadata),
        json!({
            "session_code": short_session_code(meeting.id),
            "sync_revision": session_content_revision(meeting, responses, state),
            "summary": summary,
            "conversation_memory": {
                "schema_version": CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION,
                "state": &conversation_memory,
            },
            "action_items": meeting.action_items.len(),
            "decisions": meeting.decisions.len(),
            "diagnostics": {
                "listen_runs": meeting.diagnostics.listen_runs,
                "stt_parse_errors": meeting.diagnostics.stt_parse_errors,
                "stt_provider_errors": meeting.diagnostics.stt_provider_errors,
                "audio_start_errors": meeting.diagnostics.audio_start_errors,
                "audio_source_errors": meeting.diagnostics.audio_source_errors,
                "last_audio_session_id": meeting.diagnostics.last_audio_session_id.as_deref(),
                "last_stt_provider": meeting.diagnostics.last_stt_provider.as_deref(),
                "last_error_kind": meeting.diagnostics.last_error_kind.as_deref(),
                "last_error_message_chars": meeting
                    .diagnostics
                    .last_error_message
                    .as_deref()
                    .map(str::chars)
                    .map(Iterator::count),
                "last_error_at": meeting.diagnostics.last_error_at.as_deref(),
            },
        }),
    );
    SyncSessionRecord {
        session_id,
        title: meeting.title.clone(),
        status: if meeting.ended_at.is_some() {
            "archived".into()
        } else {
            "active".into()
        },
        created_at_ms: parse_ms(&meeting.started_at),
        updated_at_ms,
        last_active_at_ms: Some(updated_at_ms),
        answer_style: answer_instructions.map(ToString::to_string),
        metadata,
        deleted_at_ms: None,
    }
}

fn cloud_diagnostics_from_metadata(metadata: &Value) -> MeetingDiagnostics {
    let mut diagnostics: MeetingDiagnostics = metadata
        .get("diagnostics")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    // Backward-compatible cloud records may still contain this legacy field.
    // Never restore it into the local private diagnostic record.
    diagnostics.last_error_message = None;
    diagnostics
}

fn conversation_memory_from_session_metadata(metadata: &Value) -> ConversationMemory {
    let Some(memory) = metadata.get("conversation_memory") else {
        return ConversationMemory::default();
    };
    let schema_version = memory
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if schema_version != u64::from(CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION) {
        warn!(
            schema_version,
            "cloud conversation memory used an unsupported schema version"
        );
        return ConversationMemory::default();
    }
    let Some(state) = memory.get("state") else {
        warn!("cloud conversation memory was missing its state payload");
        return ConversationMemory::default();
    };
    let Ok(memory) = serde_json::from_value::<ConversationMemory>(state.clone()) else {
        warn!("cloud conversation memory payload was invalid");
        return ConversationMemory::default();
    };
    if !cloud_conversation_memory_is_valid(&memory) {
        warn!("cloud conversation memory failed bounded integrity validation");
        return ConversationMemory::default();
    }
    memory
}

fn cloud_conversation_memory_is_valid(memory: &ConversationMemory) -> bool {
    if memory.epochs.is_empty() {
        return memory.revision == 0 && memory.compacted_turn_count == 0;
    }
    if memory.epochs.len() > MAX_CLOUD_CONVERSATION_MEMORY_EPOCHS
        || memory.revision == 0
        || memory.compacted_turn_count == 0
    {
        return false;
    }

    let mut previous_revision = 0_u64;
    let mut compacted_turn_count = 0_u64;
    for epoch in &memory.epochs {
        if epoch.revision == 0
            || epoch.revision < previous_revision
            || epoch.revision > memory.revision
            || epoch.turn_count == 0
            || epoch.summary.trim().is_empty()
            || epoch.summary.chars().count() > MAX_CLOUD_CONVERSATION_MEMORY_EPOCH_CHARS
            || epoch.first_created_at.chars().count()
                > MAX_CLOUD_CONVERSATION_MEMORY_TIMESTAMP_CHARS
            || epoch.last_created_at.chars().count() > MAX_CLOUD_CONVERSATION_MEMORY_TIMESTAMP_CHARS
        {
            return false;
        }
        let Some(total) = compacted_turn_count.checked_add(epoch.turn_count) else {
            return false;
        };
        compacted_turn_count = total;
        previous_revision = epoch.revision;
    }
    compacted_turn_count == memory.compacted_turn_count
}

fn transcript_record(
    meeting: &MeetingRecord,
    segment: &TranscriptSegment,
    state: Option<&CloudSyncState>,
) -> SyncTranscriptSegment {
    let preserved = state.and_then(|state| state.transcript_segments.get(&segment.id.to_string()));
    SyncTranscriptSegment {
        segment_id: wire_transcript_segment_id(meeting, segment.id, state),
        session_id: wire_session_id(meeting, state),
        speaker: preserved
            .map(|record| record.speaker.clone())
            .unwrap_or_else(|| segment.speaker.to_string()),
        // Local MeetingRecord stores speaker but not the capture source. Keep
        // unknown explicit for local-only rows instead of inventing microphone
        // or system provenance from the speaker label.
        source: preserved
            .map(|record| record.source.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        text: truncate_chars(&segment.text, MAX_TEXT_PREVIEW_CHARS),
        start_ms: preserved.and_then(|record| record.start_ms),
        end_ms: preserved.and_then(|record| record.end_ms),
        ts_ms: preserved
            .map(|record| record.ts_ms)
            .unwrap_or_else(|| parse_ms(&segment.created_at)),
        is_final: segment.is_final,
        deleted_at_ms: None,
        metadata: preserved
            .map(|record| record.metadata.clone())
            .unwrap_or_else(empty_metadata),
    }
}

fn context_record(
    meeting: &MeetingRecord,
    artifact: &ContextArtifact,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
    state: Option<&CloudSyncState>,
) -> SyncContextArtifactRecord {
    let uploaded = uploaded_objects.get(&artifact.id);
    let preserved = state.and_then(|state| state.context_artifacts.get(&artifact.id.to_string()));
    let artifact_id = wire_context_artifact_id(meeting, artifact.id, state);
    let mut current_metadata = json!({
        "size_bytes": artifact.size_bytes,
        "processing_status": artifact.processing_status.to_string(),
        "processing_error": artifact.processing_error.as_deref(),
        "answer_context_role": artifact.answer_context_role,
    });
    if let Some(uploaded) = uploaded {
        current_metadata["object_key"] = Value::String(uploaded.object_key.clone());
        current_metadata["object_size_bytes"] = Value::from(uploaded.size_bytes);
        current_metadata["object_sha256"] = Value::String(uploaded.sha256.clone());
        current_metadata["object_content_type"] = Value::String(uploaded.content_type.clone());
        current_metadata["object_expires_at_ms"] = Value::from(uploaded.expires_at_ms);
    }
    let metadata = merge_metadata(
        preserved
            .map(|record| record.metadata.clone())
            .unwrap_or_else(empty_metadata),
        current_metadata,
    );
    SyncContextArtifactRecord {
        artifact_id: artifact_id.clone(),
        session_id: wire_session_id(meeting, state),
        kind: artifact.kind.to_string(),
        title: artifact.title.clone(),
        note: artifact.note.clone(),
        source_uri: Some(cloud_safe_artifact_source_uri(
            preserved
                .and_then(|record| record.source_uri.as_deref())
                .unwrap_or(&artifact.path),
            &artifact_id,
        )),
        content_hash: uploaded
            .map(|object| object.sha256.clone())
            .or_else(|| preserved.and_then(|record| record.content_hash.clone())),
        text_preview: artifact
            .text_preview
            .as_deref()
            .map(|text| truncate_chars(text, MAX_TEXT_PREVIEW_CHARS)),
        created_at_ms: parse_ms(&artifact.created_at),
        updated_at_ms: parse_ms(if artifact.updated_at.trim().is_empty() {
            &artifact.created_at
        } else {
            &artifact.updated_at
        }),
        deleted_at_ms: None,
        metadata,
    }
}

fn cloud_safe_artifact_source_uri(candidate: &str, artifact_id: &str) -> String {
    let candidate = candidate.trim();
    if let Ok(mut url) = reqwest::Url::parse(candidate) {
        if matches!(url.scheme(), "http" | "https") {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            return url.to_string();
        }
    }
    format!("bluey://artifact/{artifact_id}")
}

fn cue_response_record(
    meeting: &MeetingRecord,
    response: &crate::llm::CueResponse,
    state: Option<&CloudSyncState>,
) -> SyncCueResponseRecord {
    let session_id = wire_session_id(meeting, state);
    let response_id = stable_wire_record_id("response", &session_id, &response.id);
    let mut metadata = json!({
        "input_tokens": response.input_tokens,
        "output_tokens": response.output_tokens,
    });
    if response.artifact_type.is_some() || response.artifact_body.is_some() {
        metadata["canvas_artifact_id"] = Value::String(
            stable_entity_uuid("canvas-artifact", &session_id, &response_id).to_string(),
        );
    }
    SyncCueResponseRecord {
        response_id,
        // The response DB is queried per meeting. Bind its parent to that
        // authenticated upload unit instead of trusting a stale source field.
        session_id,
        kind: response.kind.clone(),
        text: truncate_chars(&response.text, MAX_RESPONSE_CHARS),
        source_text: response
            .source_text
            .as_deref()
            .map(|text| truncate_chars(text, MAX_TEXT_PREVIEW_CHARS)),
        ts_ms: response.ts_ms as i64,
        provider: response.provider.clone(),
        model: response.model.clone(),
        lane: None,
        task_type: None,
        cost_cents: response.cost_cents,
        balance_cents_after: response.balance_cents_after,
        cost_label: response.cost_label.clone(),
        artifact_type: response.artifact_type.clone(),
        artifact_body: response
            .artifact_body
            .as_deref()
            .map(|body| truncate_chars(body, MAX_RESPONSE_CHARS)),
        artifact_confidence: response.artifact_confidence,
        deleted_at_ms: None,
        metadata,
    }
}

fn conversation_response_record(
    meeting: &MeetingRecord,
    turn: &ConversationTurn,
    state: Option<&CloudSyncState>,
) -> SyncCueResponseRecord {
    let session_id = wire_session_id(meeting, state);
    let preserved = state.and_then(|state| state.responses.get(&turn.id.to_string()));
    let response_id = preserved
        .map(|record| stable_wire_record_id("response", &session_id, &record.record_id))
        .unwrap_or_else(|| format!("turn-{}", turn.id));
    let attachment_ids = turn
        .attachment_ids
        .iter()
        .map(|id| wire_context_artifact_id(meeting, *id, state))
        .collect::<Vec<_>>();
    let mut metadata = merge_metadata(
        preserved
            .map(|record| record.metadata.clone())
            .unwrap_or_else(empty_metadata),
        json!({
            "turn_id": turn.id.to_string(),
            "source": turn.source.as_deref(),
            "attachment_ids": attachment_ids,
        }),
    );
    if let Some(artifact) = &turn.artifact {
        let canvas_id = metadata
            .get("canvas_artifact_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                stable_entity_uuid("canvas-artifact", &session_id, &response_id).to_string()
            });
        metadata["canvas_artifact_id"] = Value::String(canvas_id);
        metadata["canvas_artifact_title"] = Value::String(artifact.title.clone());
    }
    SyncCueResponseRecord {
        response_id,
        session_id,
        kind: preserved
            .map(|record| record.kind.clone())
            .unwrap_or_else(|| "answer".into()),
        text: truncate_chars(&turn.answer, MAX_RESPONSE_CHARS),
        source_text: Some(truncate_chars(&turn.question, MAX_TEXT_PREVIEW_CHARS)),
        ts_ms: preserved
            .map(|record| record.ts_ms)
            .unwrap_or_else(|| parse_ms(&turn.created_at)),
        provider: turn.provider.clone(),
        model: preserved.and_then(|record| record.model.clone()),
        lane: preserved.and_then(|record| record.lane.clone()),
        task_type: preserved.and_then(|record| record.task_type.clone()),
        cost_cents: preserved.and_then(|record| record.cost_cents),
        balance_cents_after: preserved.and_then(|record| record.balance_cents_after),
        cost_label: preserved.and_then(|record| record.cost_label.clone()),
        artifact_type: turn
            .artifact
            .as_ref()
            .map(|artifact| cloud_artifact_type_value(artifact.artifact_type).to_string()),
        artifact_body: turn
            .artifact
            .as_ref()
            .map(|artifact| truncate_chars(&artifact.body, MAX_RESPONSE_CHARS)),
        artifact_confidence: turn.artifact.as_ref().map(|artifact| artifact.confidence),
        deleted_at_ms: None,
        metadata,
    }
}

fn cloud_artifact_type_value(artifact_type: CardArtifactType) -> &'static str {
    match artifact_type {
        CardArtifactType::Code => "code",
        CardArtifactType::SystemDesign => "system_design",
        CardArtifactType::Screen => "screen",
        CardArtifactType::Document => "document",
        CardArtifactType::Structured => "structured",
    }
}

fn transcript_rag_chunk(
    meeting: &MeetingRecord,
    segment: &TranscriptSegment,
    state: Option<&CloudSyncState>,
) -> Option<SyncRagChunkRecord> {
    let session_id = wire_session_id(meeting, state);
    let segment_id = wire_transcript_segment_id(meeting, segment.id, state);
    Some(SyncRagChunkRecord {
        chunk_id: format!("{session_id}:transcript:{segment_id}:0"),
        session_id: Some(session_id),
        source_kind: "transcript".into(),
        source_id: segment_id,
        chunk_index: 0,
        text: truncate_nonempty(&segment.text)?,
        embedding: None,
        embedding_model: None,
        token_count: None,
        content_hash: None,
        updated_at_ms: parse_ms(&segment.created_at),
        deleted_at_ms: None,
        metadata: json!({ "speaker": segment.speaker.to_string() }),
    })
}

fn context_rag_chunk(
    meeting: &MeetingRecord,
    artifact: &ContextArtifact,
    state: Option<&CloudSyncState>,
) -> Option<SyncRagChunkRecord> {
    let session_id = wire_session_id(meeting, state);
    let artifact_id = wire_context_artifact_id(meeting, artifact.id, state);
    Some(SyncRagChunkRecord {
        chunk_id: format!("{session_id}:context:{artifact_id}:0"),
        session_id: Some(session_id),
        source_kind: "context".into(),
        source_id: artifact_id,
        chunk_index: 0,
        text: truncate_nonempty(artifact.text_preview.as_deref()?)?,
        embedding: None,
        embedding_model: None,
        token_count: None,
        content_hash: None,
        updated_at_ms: parse_ms(if artifact.updated_at.trim().is_empty() {
            &artifact.created_at
        } else {
            &artifact.updated_at
        }),
        deleted_at_ms: None,
        metadata: json!({ "title": artifact.title, "kind": artifact.kind.to_string() }),
    })
}

fn response_rag_chunk(
    meeting: &MeetingRecord,
    response: &crate::llm::CueResponse,
    response_id: &str,
    state: Option<&CloudSyncState>,
) -> Option<SyncRagChunkRecord> {
    let session_id = wire_session_id(meeting, state);
    Some(SyncRagChunkRecord {
        chunk_id: format!("{session_id}:response:{response_id}:0"),
        session_id: Some(session_id),
        source_kind: "response".into(),
        source_id: response_id.to_string(),
        chunk_index: 0,
        text: truncate_nonempty(&response.text)?,
        embedding: None,
        embedding_model: None,
        token_count: None,
        content_hash: None,
        updated_at_ms: response.ts_ms as i64,
        deleted_at_ms: None,
        metadata: json!({
            "kind": response.kind.as_str(),
            "artifact_type": response.artifact_type.as_deref(),
        }),
    })
}

fn conversation_rag_chunk(
    meeting: &MeetingRecord,
    turn: &ConversationTurn,
    state: Option<&CloudSyncState>,
) -> Option<SyncRagChunkRecord> {
    let session_id = wire_session_id(meeting, state);
    Some(SyncRagChunkRecord {
        chunk_id: format!("{session_id}:conversation:{}:0", turn.id),
        session_id: Some(session_id),
        source_kind: "conversation".into(),
        source_id: turn.id.to_string(),
        chunk_index: 0,
        text: truncate_nonempty(&format!(
            "Question: {}\nAnswer: {}",
            turn.question, turn.answer
        ))?,
        embedding: None,
        embedding_model: None,
        token_count: None,
        content_hash: None,
        updated_at_ms: parse_ms(&turn.created_at),
        deleted_at_ms: None,
        metadata: json!({ "provider": turn.provider.as_deref() }),
    })
}

fn updated_at_ms(meeting: &MeetingRecord, responses: Option<&[crate::llm::CueResponse]>) -> i64 {
    let mut latest = parse_ms(&meeting.started_at);
    if let Some(ended_at) = meeting.ended_at.as_deref() {
        latest = latest.max(parse_ms(ended_at));
    }
    for segment in &meeting.transcript {
        latest = latest.max(parse_ms(&segment.created_at));
    }
    for artifact in &meeting.context {
        latest = latest.max(parse_ms(if artifact.updated_at.trim().is_empty() {
            &artifact.created_at
        } else {
            &artifact.updated_at
        }));
    }
    for turn in &meeting.conversation {
        latest = latest.max(parse_ms(&turn.created_at));
    }
    for epoch in &meeting.conversation_memory.epochs {
        latest = latest.max(parse_ms(&epoch.last_created_at));
    }
    for response in responses.into_iter().flatten() {
        latest = latest.max(response.ts_ms as i64);
    }
    latest
}

fn session_content_revision(
    meeting: &MeetingRecord,
    responses: Option<&[crate::llm::CueResponse]>,
    state: Option<&CloudSyncState>,
) -> String {
    let revision_input = json!({
        "title": meeting.title,
        "ended_at": meeting.ended_at,
        "transcript": meeting.transcript,
        "context": meeting.context,
        "conversation": meeting.conversation,
        "conversation_memory": meeting.conversation_memory,
        "answer_instructions": meeting.answer_instructions,
        "summary": meeting.summary,
        "responses": responses.unwrap_or(&[]),
        "preserved_cloud_state": state,
    });
    let bytes = serde_json::to_vec(&revision_input).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn parse_ms(value: &str) -> i64 {
    value.trim().parse::<i64>().unwrap_or_default()
}

fn truncate_nonempty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate_chars(trimmed, MAX_TEXT_PREVIEW_CHARS))
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(max_chars) {
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::{app_paths::AppPaths, ContextKind, Speaker};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingCloudDeleteClient {
        deleted_remote_session_ids: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl CloudSessionDeleteClient for RecordingCloudDeleteClient {
        async fn delete_cloud_session(
            &self,
            remote_session_id: &str,
        ) -> std::result::Result<(), cue_cloud_client::Error> {
            self.deleted_remote_session_ids
                .lock()
                .unwrap()
                .push(remote_session_id.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailingCloudDeleteClient {
        attempted_remote_session_ids: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl CloudSessionDeleteClient for FailingCloudDeleteClient {
        async fn delete_cloud_session(
            &self,
            remote_session_id: &str,
        ) -> std::result::Result<(), cue_cloud_client::Error> {
            self.attempted_remote_session_ids
                .lock()
                .unwrap()
                .push(remote_session_id.to_string());
            Err(cue_cloud_client::Error::Server { status: 503 })
        }
    }

    fn test_meeting_store(root: &Path) -> MeetingStore {
        let paths = AppPaths {
            data_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            runtime_dir: root.join("runtime"),
            state_file: root.join("runtime/state.json"),
            account_file: root.join("config/account.json"),
            settings_file: root.join("config/settings.json"),
        };
        paths.ensure().unwrap();
        MeetingStore::new(&paths).unwrap()
    }

    fn empty_cloud_sync_state(
        owner_account_id: impl Into<String>,
        remote_session_id: impl Into<String>,
    ) -> CloudSyncState {
        CloudSyncState {
            schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
            owner_account_id: owner_account_id.into(),
            remote_session_id: remote_session_id.into(),
            remote_summary: None,
            session_metadata: json!({}),
            transcript_segments: BTreeMap::new(),
            context_artifacts: BTreeMap::new(),
            responses: BTreeMap::new(),
            synced_transcript_records: BTreeMap::new(),
            synced_response_records: BTreeMap::new(),
            rag_chunks: BTreeMap::new(),
            attachment_transfers: BTreeMap::new(),
            child_tombstones: BTreeMap::new(),
        }
    }

    fn child_tombstone(
        kind: &str,
        child_id: impl Into<String>,
        session_id: Uuid,
    ) -> CloudChildTombstone {
        CloudChildTombstone {
            child_kind: kind.to_string(),
            child_id: child_id.into(),
            session_id: session_id.to_string(),
            deleted_at_ms: i64::MAX - 1,
            source_kind: None,
            source_id: None,
            chunk_index: None,
        }
    }

    #[test]
    fn cloud_log_identifier_hash_is_stable_and_never_contains_the_identifier() {
        let identifier = b"018f38e0-6f61-74a1-9000-private-session";
        let hash = cloud_log_identifier_hash(identifier);
        let raw_identifier = String::from_utf8_lossy(identifier);

        assert_eq!(hash.len(), 12);
        assert_eq!(hash, cloud_log_identifier_hash(identifier));
        assert_ne!(hash, cloud_log_identifier_hash(b"another-session"));
        assert!(!hash.contains(raw_identifier.as_ref()));
        assert!(!raw_identifier.contains(&hash));
        assert!(!hash.contains("private"));
    }

    #[test]
    fn hydration_summary_tracks_unique_tombstoned_session_ids_separately_from_counts() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut summary = CloudHydrationSummary::default();
        let mut seen_session_ids = HashSet::new();

        summary.record_purged_session_id(&mut seen_session_ids, first);
        summary.record_purged_session_id(&mut seen_session_ids, first);
        summary.record_purged_session_id(&mut seen_session_ids, second);
        summary.purged_deleted_sessions = 1;

        assert_eq!(summary.purged_session_ids, vec![first, second]);
        assert_eq!(summary.total_sessions(), 1);
    }

    #[test]
    fn session_sync_never_transports_or_hydrates_raw_diagnostic_messages() {
        let mut meeting = MeetingRecord::new(Some("Diagnostics".into()));
        meeting.diagnostics.record_error(
            "audio_source_error",
            "secret local path /Users/private/file",
        );

        let record = session_record(&meeting, None, None);
        let serialized = serde_json::to_string(&record.metadata).unwrap();
        assert!(!serialized.contains("secret local path"));
        assert_eq!(
            record.metadata["diagnostics"]["last_error_message_chars"],
            json!(37)
        );
        assert!(record.metadata["diagnostics"]
            .get("last_error_message")
            .is_none());

        let hydrated = cloud_diagnostics_from_metadata(&json!({
            "diagnostics": {
                "audio_source_errors": 2,
                "last_error_kind": "audio_source_error",
                "last_error_message": "legacy cloud secret",
                "last_error_at": "123"
            }
        }));
        assert_eq!(hydrated.audio_source_errors, 2);
        assert_eq!(
            hydrated.last_error_kind.as_deref(),
            Some("audio_source_error")
        );
        assert!(hydrated.last_error_message.is_none());
    }

    #[test]
    fn session_delete_purges_all_local_audit_surfaces() {
        let root = std::env::temp_dir().join(format!("bluey-audit-delete-{}", Uuid::new_v4()));
        let session_id = Uuid::new_v4();
        let owner = "acct-audit-delete";
        let scope_key = account_scope_key(owner).unwrap();
        let bundle_dir = root
            .join(SESSION_AUDIT_DIR)
            .join(&scope_key)
            .join(session_id.to_string());
        let event_dir = root
            .join(SESSION_AUDIT_EVENTS_DIR)
            .join(&scope_key)
            .join(session_id.to_string());
        let marker = root
            .join(SESSION_AUDIT_UPLOADED_DIR)
            .join(scope_key)
            .join(format!("{session_id}.json"));
        fs::create_dir_all(&bundle_dir).unwrap();
        fs::create_dir_all(&event_dir).unwrap();
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(bundle_dir.join("bundle.json"), b"{}").unwrap();
        fs::write(event_dir.join("events.jsonl"), b"{}\n").unwrap();
        fs::write(&marker, b"{}").unwrap();

        purge_session_audit_state(&root, Some(owner), session_id).unwrap();
        assert!(!bundle_dir.exists());
        assert!(!event_dir.exists());
        assert!(!marker.exists());
        // Repeated deletion is idempotent.
        purge_session_audit_state(&root, Some(owner), session_id).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_purge_preserves_same_session_id_for_other_account() {
        let root = std::env::temp_dir().join(format!("bluey-audit-owner-{}", Uuid::new_v4()));
        let session_id = Uuid::new_v4();
        let scope_a = SessionAuditScope {
            session_id,
            owner_account_id: Some("acct-audit-a".into()),
        };
        let scope_b = SessionAuditScope {
            session_id,
            owner_account_id: Some("acct-audit-b".into()),
        };
        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &scope_a,
            &[("owner_a".into(), json!({"count": 1}))],
        )
        .unwrap();
        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &scope_b,
            &[("owner_b".into(), json!({"count": 1}))],
        )
        .unwrap();
        let path_a = session_audit_event_dir(&root, &scope_a);
        let path_b = session_audit_event_dir(&root, &scope_b);
        assert_ne!(path_a, path_b);

        purge_session_audit_state(&root, Some("acct-audit-a"), session_id).unwrap();
        assert!(!path_a.exists());
        assert!(path_b.join("events.jsonl").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cloud_state_outbox_and_restored_paths_are_account_scoped() {
        let root = std::env::temp_dir().join(format!("bluey-account-scope-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let session_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let owner_a = "acct-scope-a";
        let owner_b = "acct-scope-b";
        let state_a = empty_cloud_sync_state(owner_a, "remote-a");
        let state_b = empty_cloud_sync_state(owner_b, "remote-b");
        write_cloud_sync_state(&root, owner_a, session_id, &state_a).unwrap();
        write_cloud_sync_state(&root, owner_b, session_id, &state_b).unwrap();

        assert_eq!(
            load_cloud_sync_state(&root, owner_a, session_id)
                .unwrap()
                .remote_session_id,
            "remote-a"
        );
        assert_eq!(
            load_cloud_sync_state(&root, owner_b, session_id)
                .unwrap()
                .remote_session_id,
            "remote-b"
        );
        let state_path_a = cloud_sync_state_path(&root, owner_a, session_id).unwrap();
        let state_path_b = cloud_sync_state_path(&root, owner_b, session_id).unwrap();
        assert_ne!(state_path_a, state_path_b);
        assert!(!state_path_a.to_string_lossy().contains(owner_a));
        assert!(!state_path_b.to_string_lossy().contains(owner_b));

        prepare_cloud_session_delete(&root, session_id, owner_a).unwrap();
        prepare_cloud_session_delete(&root, session_id, owner_b).unwrap();
        assert_ne!(
            cloud_delete_outbox_path(&root, owner_a, session_id).unwrap(),
            cloud_delete_outbox_path(&root, owner_b, session_id).unwrap()
        );

        let record = SyncContextArtifactRecord {
            artifact_id: artifact_id.to_string(),
            session_id: session_id.to_string(),
            kind: "document".into(),
            title: "Notes.txt".into(),
            note: None,
            source_uri: None,
            content_hash: None,
            text_preview: Some("private preview".into()),
            created_at_ms: 1,
            updated_at_ms: 1,
            deleted_at_ms: None,
            metadata: json!({}),
        };
        let restored_a =
            write_restored_context_preview(&root, owner_a, session_id, artifact_id, &record)
                .unwrap();
        let restored_b =
            write_restored_context_preview(&root, owner_b, session_id, artifact_id, &record)
                .unwrap();
        assert_ne!(restored_a, restored_b);
        assert!(!restored_a.to_string_lossy().contains(owner_a));
        assert!(!restored_b.to_string_lossy().contains(owner_b));
        purge_cloud_session_local_state(&root, owner_a, session_id).unwrap();
        assert!(load_cloud_sync_state(&root, owner_a, session_id).is_none());
        assert!(load_cloud_sync_state(&root, owner_b, session_id).is_some());
        assert!(!restored_a.exists());
        assert!(restored_b.exists());
        assert!(!cloud_delete_outbox_path(&root, owner_a, session_id)
            .unwrap()
            .exists());
        assert!(cloud_delete_outbox_path(&root, owner_b, session_id)
            .unwrap()
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn account_purge_removes_only_the_selected_cloud_scope() {
        let root = std::env::temp_dir().join(format!("bluey-account-purge-{}", Uuid::new_v4()));
        let session_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let owner_a = "acct-purge-a";
        let owner_b = "acct-purge-b";
        for owner in [owner_a, owner_b] {
            write_cloud_sync_state(
                &root,
                owner,
                session_id,
                &empty_cloud_sync_state(owner, session_id.to_string()),
            )
            .unwrap();
            prepare_cloud_session_delete(&root, session_id, owner).unwrap();
            write_cloud_hydration_cursor(&root, owner, "opaque-cursor").unwrap();
            let record = SyncContextArtifactRecord {
                artifact_id: artifact_id.to_string(),
                session_id: session_id.to_string(),
                kind: "document".into(),
                title: "Private.txt".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: Some("private preview".into()),
                created_at_ms: 1,
                updated_at_ms: 1,
                deleted_at_ms: None,
                metadata: json!({}),
            };
            write_restored_context_preview(&root, owner, session_id, artifact_id, &record).unwrap();
            let scope = SessionAuditScope {
                session_id,
                owner_account_id: Some(owner.to_string()),
            };
            append_privacy_safe_diagnostic_events_for_scope(
                &root,
                &scope,
                &[("scope_test".into(), json!({"count": 1}))],
            )
            .unwrap();
        }

        purge_cloud_account_local_state(&root, owner_a).unwrap();

        assert!(load_cloud_sync_state(&root, owner_a, session_id).is_none());
        assert!(load_cloud_hydration_cursor(&root, owner_a).is_none());
        assert!(!cloud_delete_outbox_path(&root, owner_a, session_id)
            .unwrap()
            .exists());
        assert!(load_cloud_sync_state(&root, owner_b, session_id).is_some());
        assert_eq!(
            load_cloud_hydration_cursor(&root, owner_b).as_deref(),
            Some("opaque-cursor")
        );
        assert!(cloud_delete_outbox_path(&root, owner_b, session_id)
            .unwrap()
            .exists());
        assert!(session_audit_event_dir(
            &root,
            &SessionAuditScope {
                session_id,
                owner_account_id: Some(owner_b.to_string()),
            }
        )
        .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn changed_only_fingerprint_includes_rag_and_child_deletions() {
        let root =
            std::env::temp_dir().join(format!("bluey-summary-fingerprint-{}", Uuid::new_v4()));
        let owner = "acct-summary";
        let session_id = Uuid::new_v4();
        let mut state = empty_cloud_sync_state(owner, session_id.to_string());
        let mut summary = CloudSessionSummary {
            session_id: session_id.to_string(),
            title: "Session".into(),
            status: "ended".into(),
            updated_at_ms: 100,
            last_active_at_ms: None,
            answer_style: None,
            transcript_count: 2,
            response_count: 1,
            context_count: 1,
            rag_count: 4,
            child_tombstone_count: 0,
            child_tombstone_updated_at_ms: None,
        };
        state.remote_summary = Some(cloud_session_fingerprint(&summary));
        write_cloud_sync_state(&root, owner, session_id, &state).unwrap();
        assert!(cloud_session_summary_is_current(
            &root, owner, session_id, &summary
        ));

        summary.child_tombstone_count = 1;
        summary.child_tombstone_updated_at_ms = Some(110);
        assert!(!cloud_session_summary_is_current(
            &root, owner, session_id, &summary
        ));
        summary.child_tombstone_count = 0;
        summary.child_tombstone_updated_at_ms = None;
        summary.rag_count -= 1;
        assert!(!cloud_session_summary_is_current(
            &root, owner, session_id, &summary
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_unscoped_cloud_sync_state_fails_closed() {
        let root = std::env::temp_dir().join(format!("bluey-legacy-state-{}", Uuid::new_v4()));
        let session_id = Uuid::new_v4();
        let legacy_dir = root.join(CLOUD_SYNC_STATE_DIR);
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(
            legacy_dir.join(format!("{session_id}.json")),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "remote_session_id": "legacy-remote",
                "session_metadata": {},
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(load_cloud_sync_state(&root, "acct-current", session_id).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn attachment_retry_is_durable_and_owner_scoped() {
        let root = std::env::temp_dir().join(format!("bluey-object-retry-{}", Uuid::new_v4()));
        let owner = "acct-object-retry";
        let mut meeting = MeetingRecord::new(Some("Attachment retry".into()));
        meeting.owner_account_id = Some(owner.to_string());
        let artifact = ContextArtifact::new(
            ContextKind::Document,
            root.join("missing.pdf").to_string_lossy().to_string(),
            "Missing.pdf",
            None,
            Some(10),
        );
        let artifact_id = artifact.id;
        meeting.context.push(artifact);
        let mut states = HashMap::new();

        record_attachment_transfer_retry(
            &root,
            owner,
            &meeting,
            None,
            &mut states,
            artifact_id,
            artifact_id.to_string(),
            CloudAttachmentTransferStatus::UploadRetry,
            "local_file_unavailable",
        )
        .unwrap();

        let persisted = load_cloud_sync_state(&root, owner, meeting.id).unwrap();
        let transfer = persisted
            .attachment_transfers
            .get(&artifact_id.to_string())
            .unwrap();
        assert_eq!(transfer.status, CloudAttachmentTransferStatus::UploadRetry);
        assert_eq!(
            transfer.last_error_category.as_deref(),
            Some("local_file_unavailable")
        );
        assert!(load_cloud_sync_state(&root, "acct-other", meeting.id).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn owned_session_delete_is_conservatively_queued_without_sync_state() {
        let root = std::env::temp_dir().join(format!("bluey-local-delete-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let session_id = Uuid::new_v4();
        let disposition = prepare_cloud_session_delete(&root, session_id, "acct-local").unwrap();
        assert_eq!(disposition, CloudSessionDeleteDisposition::Queued);
        let queued: PendingCloudSessionDelete = serde_json::from_slice(
            &fs::read(cloud_delete_outbox_path(&root, "acct-local", session_id).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(queued.remote_session_id, session_id.to_string());
        assert_eq!(queued.state, CloudSessionDeleteState::Prepared);
        abort_prepared_cloud_session_delete(&root, session_id, "acct-local").unwrap();
        assert!(!cloud_delete_outbox_path(&root, "acct-local", session_id)
            .unwrap()
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uploaded_session_delete_uses_durable_remote_provenance() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-delete-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let session_id = Uuid::new_v4();
        write_cloud_sync_state(
            &root,
            "acct-1",
            session_id,
            &empty_cloud_sync_state("acct-1", "remote-session-1"),
        )
        .unwrap();

        let disposition = prepare_cloud_session_delete(&root, session_id, "acct-1").unwrap();
        assert_eq!(disposition, CloudSessionDeleteDisposition::Queued);
        let queued: PendingCloudSessionDelete = serde_json::from_slice(
            &fs::read(cloud_delete_outbox_path(&root, "acct-1", session_id).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(queued.local_session_id, session_id);
        assert_eq!(queued.remote_session_id, "remote-session-1");
        assert_eq!(queued.owner_account_id, "acct-1");
        assert_eq!(queued.state, CloudSessionDeleteState::Prepared);
        assert!(cloud_sync_state_path(&root, "acct-1", session_id)
            .unwrap()
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_outbox_record_remains_committed_after_state_machine_upgrade() {
        let session_id = Uuid::new_v4();
        let legacy = json!({
            "schema_version": LEGACY_CLOUD_DELETE_OUTBOX_SCHEMA_VERSION,
            "local_session_id": session_id,
            "remote_session_id": "legacy-remote",
            "owner_account_id": "acct-legacy",
            "queued_at_ms": 42,
        });
        let pending: PendingCloudSessionDelete = serde_json::from_value(legacy).unwrap();
        assert_eq!(pending.state, CloudSessionDeleteState::Committed);
        assert!(pending.committed_at_ms.is_none());
        assert_eq!(pending.attempt_count, 0);
        assert_eq!(pending.next_retry_at_ms, 0);
        assert!(pending.last_error_category.is_none());
        validate_cloud_delete_intent(&pending, session_id, "acct-legacy").unwrap();
    }

    #[test]
    fn legacy_delete_outbox_migrates_only_for_its_recorded_owner() {
        let root = std::env::temp_dir().join(format!("bluey-legacy-outbox-{}", Uuid::new_v4()));
        let legacy_root = root.join(CLOUD_DELETE_OUTBOX_DIR);
        fs::create_dir_all(&legacy_root).unwrap();
        let session_id = Uuid::new_v4();
        let pending = PendingCloudSessionDelete {
            schema_version: CLOUD_DELETE_OUTBOX_SCHEMA_VERSION,
            local_session_id: session_id,
            remote_session_id: "remote-legacy".into(),
            owner_account_id: "acct-legacy-owner".into(),
            queued_at_ms: 42,
            state: CloudSessionDeleteState::Committed,
            committed_at_ms: Some(43),
            attempt_count: 0,
            next_retry_at_ms: 0,
            last_error_category: None,
        };
        let legacy_path = legacy_root.join(format!("{session_id}.json"));
        fs::write(&legacy_path, serde_json::to_vec(&pending).unwrap()).unwrap();

        migrate_legacy_cloud_delete_intents(&root, "acct-other").unwrap();
        assert!(legacy_path.exists());
        assert!(!cloud_delete_outbox_path(&root, "acct-other", session_id)
            .unwrap()
            .exists());

        migrate_legacy_cloud_delete_intents(&root, "acct-legacy-owner").unwrap();
        assert!(!legacy_path.exists());
        let scoped = cloud_delete_outbox_path(&root, "acct-legacy-owner", session_id).unwrap();
        assert_eq!(read_cloud_delete_intent(&scoped).unwrap(), Some(pending));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn failed_local_delete_and_failed_abort_leave_inert_prepared_intent() {
        let root = std::env::temp_dir().join(format!("bluey-delete-rollback-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let mut meeting = MeetingRecord::new(Some("Keep local".into()));
        meeting.owner_account_id = Some("acct-rollback".into());
        let session_id = meeting.id;
        store.save_archived(&meeting).unwrap();

        prepare_cloud_session_delete(&root, session_id, "acct-rollback").unwrap();
        let cleanup_error =
            abort_prepared_cloud_session_delete_with(&root, session_id, "acct-rollback", |_path| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected cancellation failure",
                ))
            })
            .expect_err("injected cancellation failure should surface");
        assert!(cleanup_error
            .to_string()
            .contains("remove prepared cloud deletion"));

        // The canonical MeetingStore record remaining models a failed local
        // delete/rollback. The durable record is still prepared, so neither
        // cleanup failure nor any background flusher can promote it.
        let client = RecordingCloudDeleteClient::default();
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-rollback"), &|| Ok(()))
            .await
            .unwrap();
        assert!(client.deleted_remote_session_ids.lock().unwrap().is_empty());
        let pending = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-rollback", session_id).unwrap(),
        )
        .unwrap()
        .expect("prepared intent remains durable");
        assert_eq!(pending.state, CloudSessionDeleteState::Prepared);
        assert!(store.load_by_id(session_id).unwrap().is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_delete_is_cancelled_when_canonical_session_still_exists() {
        let root = std::env::temp_dir().join(format!("bluey-delete-reconcile-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let mut meeting = MeetingRecord::new(Some("Still local".into()));
        meeting.owner_account_id = Some("acct-reconcile".into());
        let session_id = meeting.id;
        store.save_archived(&meeting).unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-reconcile").unwrap();

        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-reconcile")).unwrap();

        assert!(store.load_by_id(session_id).unwrap().is_some());
        assert!(
            !cloud_delete_outbox_path(&root, "acct-reconcile", session_id)
                .unwrap()
                .exists()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn prepared_delete_is_promoted_after_crash_following_local_delete() {
        let root = std::env::temp_dir().join(format!("bluey-delete-recover-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let mut meeting = MeetingRecord::new(Some("Interrupted delete".into()));
        meeting.owner_account_id = Some("acct-recover".into());
        let session_id = meeting.id;
        store.save_archived(&meeting).unwrap();
        write_cloud_sync_state(
            &root,
            "acct-recover",
            session_id,
            &empty_cloud_sync_state("acct-recover", "remote-recover-1"),
        )
        .unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-recover").unwrap();
        assert!(store.delete(session_id).unwrap());

        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-recover")).unwrap();
        let recovered = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-recover", session_id).unwrap(),
        )
        .unwrap()
        .expect("reconciled delete intent");
        assert_eq!(recovered.state, CloudSessionDeleteState::Committed);

        let client = RecordingCloudDeleteClient::default();
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-recover"), &|| Ok(()))
            .await
            .unwrap();
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-recover"), &|| Ok(()))
            .await
            .unwrap();
        assert_eq!(
            *client.deleted_remote_session_ids.lock().unwrap(),
            vec!["remote-recover-1".to_string()]
        );
        assert!(!cloud_delete_outbox_path(&root, "acct-recover", session_id)
            .unwrap()
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_delete_for_another_account_remains_inert() {
        let root = std::env::temp_dir().join(format!("bluey-delete-owner-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let session_id = Uuid::new_v4();
        prepare_cloud_session_delete(&root, session_id, "acct-original").unwrap();

        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-other")).unwrap();

        let pending = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-original", session_id).unwrap(),
        )
        .unwrap()
        .expect("other-account prepared intent remains");
        assert_eq!(pending.state, CloudSessionDeleteState::Prepared);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_delete_reconciles_when_owner_becomes_available_after_startup() {
        let root = std::env::temp_dir().join(format!("bluey-delete-login-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let session_id = Uuid::new_v4();
        prepare_cloud_session_delete(&root, session_id, "acct-login").unwrap();

        // A signed-out startup cannot safely assign the intent to an account.
        reconcile_prepared_cloud_session_deletes(&root, &store, None).unwrap();
        let prepared = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-login", session_id).unwrap(),
        )
        .unwrap()
        .expect("signed-out startup leaves the intent inert");
        assert_eq!(prepared.state, CloudSessionDeleteState::Prepared);

        // The post-login reconciliation path can now prove both ownership and
        // the absence of the canonical local record.
        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-login")).unwrap();
        let committed = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-login", session_id).unwrap(),
        )
        .unwrap()
        .expect("post-login reconciliation promotes the intent");
        assert_eq!(committed.state, CloudSessionDeleteState::Committed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconciliation_waits_for_in_flight_local_delete_transaction() {
        let root =
            std::env::temp_dir().join(format!("bluey-delete-transaction-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let mut meeting = MeetingRecord::new(Some("Deleting now".into()));
        meeting.owner_account_id = Some("acct-transaction".into());
        let session_id = meeting.id;
        store.save_archived(&meeting).unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-transaction").unwrap();

        let transaction = lock_cloud_session_delete_transaction();
        let thread_root = root.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            reconcile_prepared_cloud_session_deletes(
                &thread_root,
                &store,
                Some("acct-transaction"),
            )
            .unwrap();
            done_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(done_rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        let pending = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-transaction", session_id).unwrap(),
        )
        .unwrap()
        .expect("in-flight intent remains prepared");
        assert_eq!(pending.state, CloudSessionDeleteState::Prepared);

        drop(transaction);
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("reconciliation completes after local transaction");
        worker.join().unwrap();
        assert!(
            !cloud_delete_outbox_path(&root, "acct-transaction", session_id)
                .unwrap()
                .exists()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn committed_delete_flushes_once_and_removes_durable_provenance() {
        let root = std::env::temp_dir().join(format!("bluey-delete-commit-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let mut meeting = MeetingRecord::new(Some("Delete locally".into()));
        meeting.owner_account_id = Some("acct-commit".into());
        let session_id = meeting.id;
        store.save_archived(&meeting).unwrap();
        write_cloud_sync_state(
            &root,
            "acct-commit",
            session_id,
            &empty_cloud_sync_state("acct-commit", "remote-delete-1"),
        )
        .unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-commit").unwrap();
        assert!(store.delete(session_id).unwrap());
        commit_prepared_cloud_session_delete(&root, session_id, "acct-commit").unwrap();
        let committed = read_cloud_delete_intent(
            &cloud_delete_outbox_path(&root, "acct-commit", session_id).unwrap(),
        )
        .unwrap()
        .expect("committed intent");
        assert_eq!(committed.state, CloudSessionDeleteState::Committed);
        assert!(committed.committed_at_ms.is_some());
        assert!(cloud_session_has_committed_delete(&root, "acct-commit", session_id).unwrap());

        let client = RecordingCloudDeleteClient::default();
        assert_eq!(
            flush_queued_cloud_session_delete_with(
                &root,
                session_id,
                &client,
                "acct-commit",
                &|| Ok(()),
            )
            .await
            .unwrap(),
            CloudSessionDeleteDisposition::Confirmed
        );
        assert_eq!(
            *client.deleted_remote_session_ids.lock().unwrap(),
            vec!["remote-delete-1".to_string()]
        );
        assert!(!cloud_delete_outbox_path(&root, "acct-commit", session_id)
            .unwrap()
            .exists());
        assert!(!cloud_sync_state_path(&root, "acct-commit", session_id)
            .unwrap()
            .exists());
        assert!(!cloud_session_has_committed_delete(&root, "acct-commit", session_id).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn failed_cloud_delete_persists_backoff_and_skips_immediate_retry() {
        let root = std::env::temp_dir().join(format!("bluey-delete-backoff-{}", Uuid::new_v4()));
        let session_id = Uuid::new_v4();
        prepare_cloud_session_delete(&root, session_id, "acct-backoff").unwrap();
        commit_prepared_cloud_session_delete(&root, session_id, "acct-backoff").unwrap();
        let client = FailingCloudDeleteClient::default();

        assert_eq!(
            flush_queued_cloud_session_delete_with(
                &root,
                session_id,
                &client,
                "acct-backoff",
                &|| Ok(()),
            )
            .await
            .unwrap(),
            CloudSessionDeleteDisposition::Queued
        );
        let path = cloud_delete_outbox_path(&root, "acct-backoff", session_id).unwrap();
        let pending = read_cloud_delete_intent(&path).unwrap().unwrap();
        assert_eq!(pending.attempt_count, 1);
        assert!(pending.next_retry_at_ms > current_epoch_ms());
        assert_eq!(pending.last_error_category.as_deref(), Some("server"));
        assert!(has_committed_cloud_session_delete(&root, "acct-backoff").unwrap());

        assert_eq!(
            flush_queued_cloud_session_delete_with(
                &root,
                session_id,
                &client,
                "acct-backoff",
                &|| Ok(()),
            )
            .await
            .unwrap(),
            CloudSessionDeleteDisposition::Queued
        );
        assert_eq!(client.attempted_remote_session_ids.lock().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_sync_batches_chunks_long_session_and_includes_rag() {
        let mut meeting = MeetingRecord::new(Some("Scale test".into()));
        meeting.answer_instructions = Some("Be concise.".into());
        for i in 0..520 {
            meeting.transcript.push(TranscriptSegment::new(
                if i % 2 == 0 {
                    Speaker::System
                } else {
                    Speaker::User
                },
                format!("segment {i} cloud searchable"),
                true,
            ));
        }

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        assert!(batches.len() > 1);
        assert!(batches
            .iter()
            .all(|batch| batch_total(batch) <= MAX_SYNC_RECORDS_PER_BATCH + 1));
        assert!(batches.iter().all(|batch| !batch.sessions.is_empty()));
        assert!(batches.iter().any(|batch| !batch.rag_chunks.is_empty()));
    }

    #[test]
    fn context_preview_becomes_cloud_artifact_and_rag_chunk() {
        let mut meeting = MeetingRecord::new(Some("Docs".into()));
        meeting.context.push(
            ContextArtifact::new(ContextKind::Document, "/tmp/a.pdf", "Spec", None, Some(10))
                .with_text_preview("the spec mentions blue ocean cache invalidation")
                .with_answer_context_role(AnswerContextRole::JobDescription),
        );

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        let batch = &batches[0];
        assert_eq!(batch.context_artifacts.len(), 1);
        assert_eq!(batch.rag_chunks.len(), 1);
        assert_eq!(batch.rag_chunks[0].source_kind, "context");
        assert_eq!(
            batch.context_artifacts[0].metadata["answer_context_role"],
            json!("job_description")
        );
    }

    #[test]
    fn cloud_context_role_metadata_defaults_to_other_and_decodes_known_roles() {
        assert_eq!(
            answer_context_role_from_metadata(&json!({})),
            AnswerContextRole::Other
        );
        assert_eq!(
            answer_context_role_from_metadata(
                &json!({ "answer_context_role": "user_confirmed_story" })
            ),
            AnswerContextRole::UserConfirmedStory
        );
        assert_eq!(
            answer_context_role_from_metadata(&json!({ "answer_context_role": "invalid" })),
            AnswerContextRole::Other
        );
    }

    #[test]
    fn context_role_merge_respects_artifact_revision() {
        let mut local = ContextArtifact::new(
            ContextKind::Document,
            "/tmp/local.pdf",
            "Local",
            None,
            Some(10),
        );
        local.updated_at = "10".to_string();
        let mut newer_cloud = local
            .clone()
            .with_answer_context_role(AnswerContextRole::CandidateResume);
        newer_cloud.updated_at = "20".to_string();
        merge_missing_context_fields(&mut local, newer_cloud);
        assert_eq!(
            local.answer_context_role,
            AnswerContextRole::CandidateResume
        );

        local.updated_at = "30".to_string();
        let mut older_cloud = local.clone();
        older_cloud.answer_context_role = AnswerContextRole::Other;
        older_cloud.updated_at = "25".to_string();
        merge_missing_context_fields(&mut local, older_cloud);
        assert_eq!(
            local.answer_context_role,
            AnswerContextRole::CandidateResume
        );
    }

    #[test]
    fn object_parent_preflight_contains_only_parent_sessions() {
        let empty = MeetingRecord::new(Some("No object".into()));
        let mut with_object = MeetingRecord::new(Some("Has object".into()));
        with_object.context.push(ContextArtifact::new(
            ContextKind::Document,
            "/tmp/spec.pdf",
            "Spec",
            None,
            Some(10),
        ));

        let batches = build_object_parent_batches(
            &[empty, with_object.clone()],
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].sessions.len(), 1);
        assert_eq!(
            batches[0].sessions[0].session_id,
            with_object.id.to_string()
        );
        assert!(batches[0].transcript_segments.is_empty());
        assert!(batches[0].cue_responses.is_empty());
        assert!(batches[0].context_artifacts.is_empty());
        assert!(batches[0].rag_chunks.is_empty());
    }

    #[test]
    fn cloud_artifact_source_uri_never_exposes_local_paths_or_url_secrets() {
        assert_eq!(
            cloud_safe_artifact_source_uri(
                "/Users/alice/Documents/private-resume.pdf",
                "artifact-1"
            ),
            "bluey://artifact/artifact-1"
        );
        assert_eq!(
            cloud_safe_artifact_source_uri(
                r"C:\Users\alice\Documents\private-resume.pdf",
                "artifact-2"
            ),
            "bluey://artifact/artifact-2"
        );
        assert_eq!(
            cloud_safe_artifact_source_uri(
                "https://example.com/spec?access_token=secret#private",
                "artifact-3"
            ),
            "https://example.com/spec"
        );
    }

    #[test]
    fn removed_context_emits_scoped_context_and_rag_tombstones() {
        let mut meeting = MeetingRecord::new(Some("Context cleanup".into()));
        let artifact =
            ContextArtifact::new(ContextKind::Document, "/tmp/a.pdf", "Spec", None, Some(10))
                .with_text_preview("private page text that must be removed");
        let artifact_id = artifact.id;
        meeting.context.push(artifact);
        let state = cloud_sync_state_after_upload(
            &meeting,
            None,
            &HashMap::new(),
            None,
            None,
            "acct-context-delete",
        );
        meeting.context.clear();
        let states = HashMap::from([(meeting.id, state)]);

        let batches = build_sync_batches_with_states(
            &[meeting.clone()],
            &HashMap::new(),
            &HashMap::new(),
            &states,
        );
        let context_tombstone = batches
            .iter()
            .flat_map(|batch| &batch.context_artifacts)
            .find(|record| record.artifact_id == artifact_id.to_string())
            .expect("context tombstone");
        assert!(context_tombstone.deleted_at_ms.is_some());
        assert!(context_tombstone.text_preview.is_none());

        let rag_tombstone = batches
            .iter()
            .flat_map(|batch| &batch.rag_chunks)
            .find(|record| record.source_id == artifact_id.to_string())
            .expect("RAG tombstone");
        assert!(rag_tombstone.deleted_at_ms.is_some());
        assert!(rag_tombstone.text.is_empty());
        assert_eq!(rag_tombstone.session_id, Some(meeting.id.to_string()));
    }

    #[test]
    fn corrupted_sync_state_recovers_deletion_provenance_from_atomic_backup() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-state-{}", Uuid::new_v4()));
        let mut meeting = MeetingRecord::new(Some("Crash-safe cleanup".into()));
        let artifact = ContextArtifact::new(
            ContextKind::Document,
            "/tmp/private.pdf",
            "Private",
            None,
            Some(10),
        )
        .with_text_preview("private content that must still be deleted");
        let artifact_id = artifact.id;
        meeting.context.push(artifact);
        let state = cloud_sync_state_after_upload(
            &meeting,
            None,
            &HashMap::new(),
            None,
            None,
            "acct-cloud-state",
        );

        write_cloud_sync_state(&root, "acct-cloud-state", meeting.id, &state)
            .expect("initial sync state");
        write_cloud_sync_state(&root, "acct-cloud-state", meeting.id, &state)
            .expect("state with backup");
        fs::write(
            cloud_sync_state_path(&root, "acct-cloud-state", meeting.id).unwrap(),
            b"{interrupted",
        )
        .expect("simulate interrupted legacy write");

        let recovered = load_cloud_sync_state(&root, "acct-cloud-state", meeting.id)
            .expect("last valid state backup");
        assert_eq!(recovered.remote_session_id, state.remote_session_id);
        meeting.context.clear();
        let states = HashMap::from([(meeting.id, recovered)]);
        let batches =
            build_sync_batches_with_states(&[meeting], &HashMap::new(), &HashMap::new(), &states);
        assert!(batches
            .iter()
            .flat_map(|batch| &batch.context_artifacts)
            .any(|record| {
                record.artifact_id == artifact_id.to_string() && record.deleted_at_ms.is_some()
            }));
        assert!(batches
            .iter()
            .flat_map(|batch| &batch.rag_chunks)
            .any(|record| record.source_id == artifact_id.to_string()
                && record.deleted_at_ms.is_some()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compacted_conversation_memory_syncs_as_derived_rag_evidence() {
        let mut meeting = MeetingRecord::new(Some("Long conversation".into()));
        for index in 0..98 {
            meeting.push_conversation_turn(ConversationTurn::new(
                format!("Question {index}"),
                format!("Answer {index}"),
                None,
                Some("test".into()),
            ));
        }

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        let memory_chunks = batches
            .iter()
            .flat_map(|batch| &batch.rag_chunks)
            .filter(|chunk| chunk.source_kind == "conversation_memory")
            .collect::<Vec<_>>();

        assert_eq!(memory_chunks.len(), 2);
        assert!(memory_chunks[0].text.contains("Question 0"));
        assert_eq!(memory_chunks[0].metadata["derived"], true);
        assert_eq!(memory_chunks[1].metadata["memory_revision"], 2);
    }

    #[tokio::test]
    async fn compacted_conversation_memory_roundtrips_through_cloud_session_metadata() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-memory-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let mut meeting = MeetingRecord::new(Some("Long conversation".into()));
        for index in 0..98 {
            meeting.push_conversation_turn(ConversationTurn::new(
                format!("Question {index}"),
                format!("Answer {index}"),
                None,
                Some("test".into()),
            ));
        }
        let original_memory = meeting.conversation_memory.clone();
        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        let session = batches[0].sessions[0].clone();
        assert_eq!(
            session.metadata["conversation_memory"]["schema_version"],
            CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION
        );
        let bundle = CloudSessionBundle {
            session,
            transcript_segments: batches
                .iter()
                .flat_map(|batch| batch.transcript_segments.clone())
                .collect(),
            cue_responses: batches
                .iter()
                .flat_map(|batch| batch.cue_responses.clone())
                .collect(),
            context_artifacts: batches
                .iter()
                .flat_map(|batch| batch.context_artifacts.clone())
                .collect(),
            rag_chunks: batches
                .iter()
                .flat_map(|batch| batch.rag_chunks.clone())
                .collect(),
            child_tombstones: Vec::new(),
        };

        let (restored, attachment_retries, _) =
            meeting_from_cloud_bundle(&root, None, "acct-memory-roundtrip", bundle, &|| Ok(()))
                .await
                .expect("restore meeting");
        assert_eq!(attachment_retries, 0);
        assert_eq!(restored.conversation_memory, original_memory);

        let uploaded_again = build_sync_batches(&[restored], &HashMap::new(), &HashMap::new());
        let restored_again =
            conversation_memory_from_session_metadata(&uploaded_again[0].sessions[0].metadata);
        assert_eq!(restored_again, original_memory);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn conversation_turn_falls_back_to_cue_response() {
        let mut meeting = MeetingRecord::new(Some("Chat".into()));
        meeting.conversation.push(ConversationTurn::new(
            "hi",
            "hello",
            None,
            Some("test".into()),
        ));

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        assert_eq!(batches[0].cue_responses.len(), 1);
        assert!(batches[0].cue_responses[0].response_id.starts_with("turn-"));
    }

    #[test]
    fn empty_meeting_shell_does_not_sync() {
        let meeting = MeetingRecord::new(Some("New recording".into()));

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());

        assert!(batches.is_empty());
    }

    #[test]
    fn cue_response_only_meeting_still_syncs_session() {
        let meeting = MeetingRecord::new(Some("Recovered chat".into()));
        let mut response_map = HashMap::new();
        response_map.insert(
            meeting.id.to_string(),
            vec![crate::llm::CueResponse::new(
                "answer",
                "Recovered answer".into(),
                &meeting.id.to_string(),
                Some("Recovered question".into()),
            )],
        );

        let batches = build_sync_batches(&[meeting], &response_map, &HashMap::new());

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].sessions.len(), 1);
        assert_eq!(batches[0].cue_responses.len(), 1);
        assert_eq!(
            batches[0].cue_responses[0].source_text.as_deref(),
            Some("Recovered question")
        );
    }

    #[test]
    fn cloud_sync_keeps_more_than_500_responses_until_explicit_removal() {
        let root = std::env::temp_dir().join(format!(
            "bluey-cloud-complete-response-history-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let owner = "acct-long-chat";
        let mut meeting = MeetingRecord::new(Some("Long chat".into()));
        meeting.owner_account_id = Some(owner.to_string());
        let session_id = meeting.id.to_string();
        let db_path = root.join("sessions.db");
        let db = Database::open(db_path.to_str().expect("utf-8 db path")).expect("open db");
        db.ensure_session_record_for_owner(Some(owner), meeting.id, &meeting.title, 1, 1)
            .expect("ensure session");

        let mut local_response_ids = Vec::new();
        for index in 0..501 {
            let response_id = format!("turn-{}", Uuid::new_v4());
            let question = format!("Question {index}");
            let answer = format!("Answer {index}");
            db.insert_cue_response(crate::db::NewCueResponse {
                id: &response_id,
                session_id: &session_id,
                kind: "answer",
                text: &answer,
                source_text: Some(&question),
                ts_ms: index + 1,
                cost_cents: None,
                balance_cents_after: None,
                provider: Some("test"),
                model: Some("test"),
                input_tokens: None,
                output_tokens: None,
                cost_label: None,
                artifact_type: None,
                artifact_body: None,
                artifact_confidence: None,
            })
            .expect("insert response");
            local_response_ids.push(response_id);
        }

        let complete_responses = db
            .list_all_cue_responses(&session_id)
            .expect("load complete response history");
        assert_eq!(complete_responses.len(), 501);
        let initial_response_map =
            HashMap::from([(session_id.clone(), complete_responses.clone())]);
        let initial_batches =
            build_sync_batches(&[meeting.clone()], &initial_response_map, &HashMap::new());
        let initial_children = uploaded_child_states_by_session(&initial_batches);
        let initial_state = cloud_sync_state_after_upload(
            &meeting,
            Some(complete_responses.as_slice()),
            &HashMap::new(),
            None,
            initial_children.get(&session_id),
            owner,
        );
        assert_eq!(initial_state.synced_response_records.len(), 501);
        drop(db);

        let loaded_response_map =
            load_local_responses(&root, owner, std::slice::from_ref(&meeting));
        assert_eq!(loaded_response_map[&session_id].len(), 501);
        let sync_states = HashMap::from([(meeting.id, initial_state.clone())]);
        let unchanged_batches = build_sync_batches_with_states(
            std::slice::from_ref(&meeting),
            &loaded_response_map,
            &HashMap::new(),
            &sync_states,
        );
        let response_tombstones = unchanged_batches
            .iter()
            .flat_map(|batch| &batch.cue_responses)
            .filter(|record| record.deleted_at_ms.is_some())
            .count();
        assert_eq!(response_tombstones, 0);

        let oldest_wire_response_id = initial_batches
            .iter()
            .flat_map(|batch| &batch.cue_responses)
            .find(|record| record.source_text.as_deref() == Some("Question 0"))
            .map(|record| record.response_id.clone())
            .expect("oldest uploaded response");
        let mut explicitly_removed_map = loaded_response_map;
        explicitly_removed_map
            .get_mut(&session_id)
            .expect("session response history")
            .retain(|response| response.id != local_response_ids[0]);
        let deletion_batches = build_sync_batches_with_states(
            std::slice::from_ref(&meeting),
            &explicitly_removed_map,
            &HashMap::new(),
            &sync_states,
        );
        let response_tombstones = deletion_batches
            .iter()
            .flat_map(|batch| &batch.cue_responses)
            .filter(|record| record.deleted_at_ms.is_some())
            .collect::<Vec<_>>();
        assert_eq!(response_tombstones.len(), 1);
        assert_eq!(response_tombstones[0].response_id, oldest_wire_response_id);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn session_audit_bundle_writes_reviewable_shape_and_marker() {
        let root = std::env::temp_dir().join(format!("bluey-audit-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("audit test root");

        let mut meeting = MeetingRecord::new(Some("Audit session".into()));
        meeting.owner_account_id = Some("acct_audit".into());
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::User,
            "Explain LRU cache",
            true,
        ));
        meeting.context.push(
            ContextArtifact::new(
                ContextKind::Image,
                "/tmp/screen.png",
                "Screen context",
                Some("Captured screen".into()),
                Some(123),
            )
            .with_text_preview("LRU cache coding question"),
        );
        meeting.conversation.push(
            ConversationTurn::new(
                "Explain LRU cache",
                "Use a hashmap plus a doubly linked list.",
                Some("manual".into()),
                Some("bluey_managed".into()),
            )
            .with_artifact(Some(CueCardArtifact {
                artifact_type: CardArtifactType::Code,
                title: "LRU cache".into(),
                body: "class LRUCache: pass".into(),
                confidence: 0.94,
            })),
        );

        let mut response = crate::llm::CueResponse::new(
            "answer",
            "Use a hashmap plus a doubly linked list.".into(),
            &meeting.id.to_string(),
            Some("Explain LRU cache".into()),
        );
        response.cost_cents = Some(3);
        response.balance_cents_after = Some(1497);
        response.provider = Some("test-provider".into());
        response.model = Some("test-model".into());
        response.input_tokens = Some(20);
        response.output_tokens = Some(40);
        response.artifact_type = Some("code".into());
        response.artifact_body = Some("class LRUCache: pass".into());
        response.artifact_confidence = Some(0.94);

        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &SessionAuditScope::from_meeting(&meeting),
            &[(
                "answer_completed".to_string(),
                json!({
                    "schema_version": 2,
                    "event_name": "answer_completed",
                    "component": "daemon",
                    "outcome": "succeeded",
                    "created_at_ms": current_epoch_ms(),
                    "monotonic_offset_ms": 0,
                    "duration_ms": 120,
                    "output_chars": response.text.chars().count(),
                    "private_probe": response.text,
                }),
            )],
        )
        .expect("append diagnostic event");
        let built = build_local_session_audit_bundle(&root, &meeting).expect("audit bundle");

        assert!(built.local_dir.join("manifest.json").is_file());
        assert!(built.local_dir.join("events.jsonl").is_file());
        assert!(built.local_dir.join("bundle.json").is_file());
        assert!(!built.local_dir.join("questions.jsonl").exists());
        assert!(!built.local_dir.join("responses.jsonl").exists());
        assert!(!built.local_dir.join("transcript.jsonl").exists());
        assert!(!built.local_dir.join("context.jsonl").exists());
        assert!(!built.local_dir.join("screen.jsonl").exists());
        assert!(!built.local_dir.join("artifacts.jsonl").exists());
        assert!(!built.local_dir.join("costs.jsonl").exists());
        assert!(!built.local_dir.join("attachments.jsonl").exists());
        assert!(!built.local_dir.join("audio").exists());
        assert_eq!(built.bundle.content_policy, "metadata_only");
        assert_eq!(built.bundle.events.len(), 1);
        let retried = build_local_session_audit_bundle(&root, &meeting)
            .expect("retry audit bundle from unchanged event log");
        assert_eq!(built.bundle.bundle_id, retried.bundle.bundle_id);
        assert_eq!(built.bytes, retried.bytes);
        let serialized = String::from_utf8(built.bytes.clone()).expect("audit bundle utf-8");
        for private_value in [
            "Explain LRU cache",
            "Use a hashmap plus a doubly linked list.",
            "class LRUCache: pass",
            "/tmp/screen.png",
            "Captured screen",
        ] {
            assert!(
                !serialized.contains(private_value),
                "diagnostic bundle leaked private content: {private_value}"
            );
        }
        assert!(serialized.contains("metadata_only"));

        let response = SessionAuditBundleResponse {
            session_id: meeting.id.to_string(),
            bundle_id: built.bundle.bundle_id.clone(),
            object_key: "prod/logs/accounts/acct_audit/session-audit.json".into(),
            size_bytes: built.bytes.len() as u64,
            sha256: "abc123".into(),
            content_type: AUDIT_BUNDLE_CONTENT_TYPE.into(),
            expires_at_ms: current_epoch_ms() + 90 * 86_400_000,
        };
        write_audit_upload_marker(&root, &meeting, &built, &response).expect("write marker");
        assert!(audit_upload_marker_matches(
            &root,
            &meeting,
            &built.bundle.bundle_id
        ));
        assert!(!audit_upload_marker_matches(
            &root,
            &meeting,
            "audit-different"
        ));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn session_audit_bundle_excludes_legacy_untyped_ui_events() {
        let root = std::env::temp_dir().join(format!("bluey-audit-events-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("audit test root");

        let mut meeting = MeetingRecord::new(Some("Visible glitch".into()));
        meeting.owner_account_id = Some("acct_events".into());
        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &SessionAuditScope::from_meeting(&meeting),
            &[(
                "ui_answer_status".to_string(),
                json!({ "message": "Reading screen context" }),
            )],
        )
        .expect("append status");
        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &SessionAuditScope::from_meeting(&meeting),
            &[(
                "ui_answer_error".to_string(),
                json!({ "visible_message": "Bluey could not complete that answer yet." }),
            )],
        )
        .expect("append error");

        let event_dir = session_audit_event_dir(&root, &SessionAuditScope::from_meeting(&meeting));
        assert!(event_dir.join("events.jsonl").is_file());

        let built = build_local_session_audit_bundle(&root, &meeting).expect("audit bundle");

        assert!(built.bundle.events.is_empty());
        let serialized = String::from_utf8(built.bytes.clone()).expect("audit bundle utf-8");
        assert!(!serialized.contains("Reading screen context"));
        assert!(!serialized.contains("Bluey could not complete that answer yet."));
        assert_eq!(
            built
                .bundle
                .manifest
                .get("record_counts")
                .and_then(|counts| counts.get("events"))
                .and_then(Value::as_u64),
            Some(0)
        );

        remove_session_audit_event_log(&root, &meeting).expect("remove event log");
        assert!(!event_dir.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn support_diagnostic_payload_rejects_secret_shaped_labels() {
        let payload = support_diagnostic_payload(json!({
            "schema_version": 2,
            "event_name": "answer_completed",
            "component": "daemon",
            "outcome": "succeeded",
            "created_at_ms": 1,
            "monotonic_offset_ms": 2,
            "provider": "private_account_name",
            "model": "gpt-secret-customer-name",
            "action": "private_secret_123",
            "error_category": "customer_email",
            "question_intent": "general",
            "artifact_type": "code"
        }));

        assert_eq!(payload["question_intent"], "general");
        assert_eq!(payload["artifact_type"], "code");
        for rejected in ["provider", "model", "action", "error_category"] {
            assert!(payload.get(rejected).is_none(), "unexpected {rejected}");
        }
    }

    #[test]
    fn cloud_session_cursor_stops_on_completion_and_rejects_cycles() {
        let mut cursor = None;
        let mut seen = HashSet::new();
        assert!(
            advance_cloud_session_cursor(&mut cursor, &mut seen, Some("page-two".to_string()))
                .unwrap()
        );
        assert_eq!(cursor.as_deref(), Some("page-two"));
        let error =
            advance_cloud_session_cursor(&mut cursor, &mut seen, Some("page-two".to_string()))
                .expect_err("a repeated server cursor must fail closed");
        assert!(error.to_string().contains("repeated cursor"));
        assert!(!advance_cloud_session_cursor(&mut cursor, &mut seen, None).unwrap());
    }

    #[test]
    fn local_audit_event_log_is_bounded_without_cloud_sync() {
        let root = std::env::temp_dir().join(format!("bluey-audit-cap-{}", Uuid::new_v4()));
        let meeting = MeetingRecord::new(Some("Bounded diagnostics".into()));
        let event_dir = session_audit_event_dir(&root, &SessionAuditScope::from_meeting(&meeting));
        std::fs::create_dir_all(&event_dir).unwrap();
        let event_path = event_dir.join("events.jsonl");
        let mut oversized_history = Vec::new();
        for sequence in 1..=(MAX_AUDIT_EVENT_RECORDS + 32) {
            serde_json::to_writer(
                &mut oversized_history,
                &json!({
                    "schema_version": AUDIT_SCHEMA_VERSION,
                    "sequence": sequence,
                    "kind": "historical",
                }),
            )
            .unwrap();
            oversized_history.push(b'\n');
        }
        std::fs::write(&event_path, oversized_history).unwrap();

        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &SessionAuditScope::from_meeting(&meeting),
            &[(
                "bounded".to_string(),
                json!({ "provider": "x".repeat(MAX_AUDIT_EVENT_RECORD_BYTES * 2) }),
            )],
        )
        .unwrap();

        let metadata = std::fs::metadata(&event_path).unwrap();
        let events = read_session_audit_events(&root, &meeting);
        assert!(metadata.len() <= MAX_AUDIT_EVENT_LOG_BYTES);
        assert!(events.len() as u64 <= MAX_AUDIT_EVENT_RECORDS);
        assert_eq!(
            events.last().and_then(|event| event["sequence"].as_u64()),
            Some(MAX_AUDIT_EVENT_RECORDS + 33)
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event["payload"]["content_policy"].as_str()),
            Some("metadata_only")
        );
        assert!(events.last().unwrap()["payload"].get("provider").is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_compaction_discards_oversized_legacy_lines_without_unbounded_reads() {
        let root = std::env::temp_dir().join(format!("bluey-audit-line-{}", Uuid::new_v4()));
        let meeting = MeetingRecord::new(Some("Bounded migration".into()));
        let event_dir = session_audit_event_dir(&root, &SessionAuditScope::from_meeting(&meeting));
        std::fs::create_dir_all(&event_dir).unwrap();
        let event_path = event_dir.join("events.jsonl");
        let mut history = vec![b'x'; MAX_AUDIT_EVENT_RECORD_BYTES * 4];
        history.push(b'\n');
        serde_json::to_writer(
            &mut history,
            &json!({
                "schema_version": AUDIT_SCHEMA_VERSION,
                "sequence": 41,
                "kind": "valid_tail",
            }),
        )
        .unwrap();
        history.push(b'\n');
        std::fs::write(&event_path, history).unwrap();

        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &SessionAuditScope::from_meeting(&meeting),
            &[("next".to_string(), json!({ "success": true }))],
        )
        .unwrap();

        let events = read_session_audit_events(&root, &meeting);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["sequence"], 41);
        assert_eq!(events[1]["sequence"], 42);
        assert!(std::fs::metadata(&event_path).unwrap().len() <= MAX_AUDIT_EVENT_LOG_BYTES);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn conversation_sync_metadata_preserves_attachment_ids() {
        let mut meeting = MeetingRecord::new(Some("Chat".into()));
        let attachment_id = Uuid::new_v4();
        let turn = ConversationTurn::new("question", "answer", None, Some("test".into()))
            .with_attachment_ids(vec![attachment_id]);

        let record = conversation_response_record(&meeting, &turn, None);
        let ids = record
            .metadata
            .get("attachment_ids")
            .and_then(|value| value.as_array())
            .expect("attachment ids");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], attachment_id.to_string());

        meeting.conversation.push(turn);
        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        assert_eq!(
            batches[0].cue_responses[0].metadata["attachment_ids"][0],
            attachment_id.to_string()
        );
    }

    #[test]
    fn reconcile_cloud_meeting_preserves_compacted_memory_and_bounds_union() {
        let mut cloud = MeetingRecord::new(Some("Cloud history".into()));
        cloud.owner_account_id = Some("acct-memory".into());
        for index in 0..98 {
            let mut turn = ConversationTurn::new(
                format!("Cloud question {index}"),
                format!("Cloud answer {index}"),
                None,
                Some("cloud".into()),
            );
            turn.created_at = index.to_string();
            cloud.push_conversation_turn(turn);
        }
        assert!(!cloud.conversation_memory.is_empty());

        let mut local = MeetingRecord::new(Some("Local partial".into()));
        local.id = cloud.id;
        local.owner_account_id = cloud.owner_account_id.clone();
        for index in 98..138 {
            let mut turn = ConversationTurn::new(
                format!("Local question {index}"),
                format!("Local answer {index}"),
                None,
                Some("local".into()),
            );
            turn.created_at = index.to_string();
            local.conversation.push(turn);
        }

        let (reconciled, changed) =
            reconcile_cloud_meeting(local, cloud, &[]).expect("reconcile conversation");
        assert!(changed);
        assert_eq!(reconciled.conversation.len(), 64);
        assert_eq!(reconciled.conversation[0].created_at, "74");
        assert_eq!(
            reconciled
                .conversation
                .last()
                .map(|turn| turn.created_at.as_str()),
            Some("137")
        );
        assert!(reconciled.conversation_memory.compacted_turn_count >= 74);
        let memory = reconciled.conversation_memory.render_bounded(8, 40_000);
        assert!(memory.contains("Cloud question 0"));
        assert!(memory.contains("Cloud question 73"));
    }

    #[test]
    fn reconcile_cloud_meeting_rejects_cross_owner_memory() {
        let mut local = MeetingRecord::new(Some("Local".into()));
        local.owner_account_id = Some("acct-a".into());
        let mut cloud = local.clone();
        cloud.owner_account_id = Some("acct-b".into());
        assert!(reconcile_cloud_meeting(local, cloud, &[]).is_err());
    }

    #[test]
    fn child_tombstones_remove_local_content_and_suppress_stale_upload() {
        let owner = "acct-child-delete";
        let mut meeting = MeetingRecord::new(Some("Deletion convergence".into()));
        meeting.owner_account_id = Some(owner.to_string());
        meeting.summary = Some("Deleted summary".into());
        meeting.answer_instructions = Some("Deleted instructions".into());
        let mut segment = TranscriptSegment::new(Speaker::User, "Deleted transcript", true);
        segment.created_at = "1".into();
        let segment_id = segment.id;
        meeting.transcript.push(segment);
        let mut context = ContextArtifact::new(
            ContextKind::Document,
            "/tmp/deleted.pdf",
            "Deleted.pdf",
            None,
            Some(10),
        );
        context.updated_at = "1".into();
        let context_id = context.id;
        meeting.context.push(context);
        for index in 0..98 {
            meeting.push_conversation_turn(ConversationTurn::new(
                format!("Memory question {index}"),
                format!("Memory answer {index}"),
                None,
                Some("test".into()),
            ));
        }
        assert!(!meeting.conversation_memory.epochs.is_empty());
        let mut deleted_turn = ConversationTurn::new(
            "Deleted question",
            "Deleted answer",
            None,
            Some("test".into()),
        )
        .with_attachment_ids(vec![context_id]);
        deleted_turn.created_at = "1".into();
        let deleted_turn_id = deleted_turn.id;
        meeting.conversation.push(deleted_turn);

        let session_id = meeting.id;
        let response_id = format!("turn-{deleted_turn_id}");
        let summary_id = format!("{session_id}:summary:0");
        let instructions_id = format!("{session_id}:instructions:0");
        let memory_id = format!("{session_id}:conversation-memory:0");
        let mut memory_tombstone = child_tombstone("rag", &memory_id, session_id);
        memory_tombstone.source_kind = Some("conversation_memory".into());
        memory_tombstone.source_id = Some(session_id.to_string());
        memory_tombstone.chunk_index = Some(0);
        let tombstones = vec![
            child_tombstone("transcript", segment_id.to_string(), session_id),
            child_tombstone("context", context_id.to_string(), session_id),
            child_tombstone("response", &response_id, session_id),
            child_tombstone("rag", &summary_id, session_id),
            child_tombstone("rag", &instructions_id, session_id),
            memory_tombstone,
        ];

        let mut reconciled = meeting.clone();
        apply_cloud_child_tombstones_to_meeting(&mut reconciled, &tombstones);
        assert!(reconciled
            .transcript
            .iter()
            .all(|segment| segment.id != segment_id));
        assert!(reconciled
            .context
            .iter()
            .all(|artifact| artifact.id != context_id));
        assert!(reconciled
            .conversation
            .iter()
            .all(|turn| turn.id != deleted_turn_id));
        assert!(reconciled.summary.is_none());
        assert!(reconciled.answer_instructions.is_none());
        assert!(reconciled.conversation_memory.is_empty());
        apply_cloud_child_tombstones_to_meeting(&mut reconciled, &tombstones);
        assert!(reconciled.conversation_memory.is_empty());

        let mut state = empty_cloud_sync_state(owner, session_id.to_string());
        state.child_tombstones = newest_cloud_child_tombstones(tombstones);
        let states = HashMap::from([(session_id, state)]);
        let batches =
            build_sync_batches_with_states(&[meeting], &HashMap::new(), &HashMap::new(), &states);
        assert!(batches.iter().all(|batch| {
            batch
                .transcript_segments
                .iter()
                .all(|record| record.segment_id != segment_id.to_string())
                && batch
                    .context_artifacts
                    .iter()
                    .all(|record| record.artifact_id != context_id.to_string())
                && batch
                    .cue_responses
                    .iter()
                    .all(|record| record.response_id != response_id)
                && batch.rag_chunks.iter().all(|record| {
                    record.chunk_id != summary_id
                        && record.chunk_id != instructions_id
                        && record.source_kind != "conversation_memory"
                })
        }));
        let session = &batches[0].sessions[0];
        assert!(session.answer_style.is_none());
        assert!(session.metadata["summary"].is_null());
        assert_eq!(
            session.metadata["conversation_memory"]["state"],
            serde_json::to_value(ConversationMemory::default()).unwrap()
        );
    }

    #[test]
    fn stale_live_save_reapplies_durable_child_tombstone_before_persistence() {
        let root = std::env::temp_dir().join(format!(
            "bluey-cloud-tombstone-save-race-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let owner = "acct-race";
        let mut stale_runtime = MeetingRecord::new(Some("Live session".into()));
        stale_runtime.owner_account_id = Some(owner.to_string());
        let deleted_turn = ConversationTurn::new(
            "deleted question",
            "deleted answer",
            None,
            Some("test".into()),
        );
        let deleted_turn_id = deleted_turn.id;
        stale_runtime.conversation.push(deleted_turn);

        // Hydration publishes deletion provenance before it acquires the
        // local persistence barrier. A live writer that retained this older
        // runtime copy must observe that provenance before saving.
        let response_id = format!("turn-{deleted_turn_id}");
        let tombstone = child_tombstone("response", &response_id, stale_runtime.id);
        let mut state = empty_cloud_sync_state(owner, stale_runtime.id.to_string());
        state.child_tombstones = newest_cloud_child_tombstones(vec![tombstone]);
        write_cloud_sync_state(&root, owner, stale_runtime.id, &state).unwrap();

        let changed =
            reapply_cloud_child_tombstones_before_save(&root, owner, &mut stale_runtime).unwrap();
        assert_eq!(changed, 1);
        assert!(stale_runtime
            .conversation
            .iter()
            .all(|turn| turn.id != deleted_turn_id));
        stale_runtime.conversation.push(ConversationTurn::new(
            "new live question",
            "new live answer",
            None,
            Some("test".into()),
        ));
        let store = test_meeting_store(&root);
        store.save_active(&stale_runtime).unwrap();
        let persisted = store.load_active().unwrap().unwrap();
        assert!(persisted
            .conversation
            .iter()
            .all(|turn| turn.id != deleted_turn_id));
        assert_eq!(persisted.conversation.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundle_allows_attachment_reference_to_context_tombstone() {
        let session_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let response_id = format!("turn-{}", Uuid::new_v4());
        let bundle = CloudSessionBundle {
            session: SyncSessionRecord {
                session_id: session_id.to_string(),
                title: "Deleted attachment".into(),
                status: "archived".into(),
                created_at_ms: 1,
                updated_at_ms: 2,
                last_active_at_ms: Some(2),
                answer_style: None,
                metadata: json!({}),
                deleted_at_ms: None,
            },
            transcript_segments: Vec::new(),
            cue_responses: vec![SyncCueResponseRecord {
                response_id,
                session_id: session_id.to_string(),
                kind: "answer".into(),
                text: "Answer".into(),
                source_text: Some("Question".into()),
                ts_ms: 1,
                provider: None,
                model: None,
                lane: None,
                task_type: None,
                cost_cents: None,
                balance_cents_after: None,
                cost_label: None,
                artifact_type: None,
                artifact_body: None,
                artifact_confidence: None,
                deleted_at_ms: None,
                metadata: json!({ "attachment_ids": [artifact_id.to_string()] }),
            }],
            context_artifacts: Vec::new(),
            rag_chunks: Vec::new(),
            child_tombstones: vec![child_tombstone(
                "context",
                artifact_id.to_string(),
                session_id,
            )],
        };

        validate_cloud_bundle_parentage(&bundle).unwrap();
    }

    #[test]
    fn conversation_sync_preserves_code_artifact_fields() {
        let meeting = MeetingRecord::new(Some("Chat".into()));
        let artifact = CueCardArtifact {
            artifact_type: CardArtifactType::Code,
            title: "Code canvas".into(),
            body: "CODE\n----\nfn main() {}".into(),
            confidence: 0.95,
        };
        let turn = ConversationTurn::new("code?", "Here is code.", None, Some("test".into()))
            .with_artifact(Some(artifact));

        let record = conversation_response_record(&meeting, &turn, None);
        assert_eq!(record.artifact_type.as_deref(), Some("code"));
        assert_eq!(
            record.artifact_body.as_deref(),
            Some("CODE\n----\nfn main() {}")
        );
        assert_eq!(record.artifact_confidence, Some(0.95));

        let restored = conversation_turn_from_cloud(record, turn.id).expect("restored turn");
        assert_eq!(
            restored
                .artifact
                .as_ref()
                .map(|artifact| artifact.artifact_type),
            Some(CardArtifactType::Code)
        );
        assert!(restored
            .artifact
            .as_ref()
            .is_some_and(|artifact| artifact.body.contains("fn main")));
    }

    #[test]
    fn cloud_delete_follows_only_the_current_owner() {
        let mut owned = MeetingRecord::new(Some("Owned".into()));
        owned.owner_account_id = Some("acct_current".into());
        let mut other = MeetingRecord::new(Some("Other".into()));
        other.owner_account_id = Some("acct_other".into());
        let unowned = MeetingRecord::new(Some("Legacy".into()));

        assert!(meeting_should_follow_cloud_delete(
            &owned,
            Some("acct_current")
        ));
        assert!(!meeting_should_follow_cloud_delete(
            &unowned,
            Some("acct_current")
        ));
        assert!(!meeting_should_follow_cloud_delete(
            &other,
            Some("acct_current")
        ));
        assert!(meeting_should_follow_cloud_delete(&unowned, None));
        assert!(!meeting_should_follow_cloud_delete(&owned, None));
    }

    #[test]
    fn cloud_delete_cleanup_removes_only_bluey_owned_context_cache() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-delete-{}", Uuid::new_v4()));
        let image_dir = root.join("context-images");
        let markdown_dir = root.join("context-markdown");
        let original_dir = root.join("originals");
        std::fs::create_dir_all(&image_dir).expect("image dir");
        std::fs::create_dir_all(&markdown_dir).expect("markdown dir");
        std::fs::create_dir_all(&original_dir).expect("original dir");

        let image_id = Uuid::new_v4();
        let doc_id = Uuid::new_v4();
        let image_path = image_dir.join(format!("{image_id}.jpg"));
        let markdown_path = markdown_dir.join(format!("{doc_id}.md"));
        let original_path = original_dir.join("Resume.pdf");
        std::fs::write(&image_path, b"image").expect("write image");
        std::fs::write(&markdown_path, b"markdown").expect("write markdown");
        std::fs::write(&original_path, b"original").expect("write original");

        let mut image = ContextArtifact::new(
            ContextKind::Image,
            image_path.to_string_lossy(),
            "Screen",
            None,
            Some(5),
        );
        image.id = image_id;
        let mut doc = ContextArtifact::new(
            ContextKind::Document,
            original_path.to_string_lossy(),
            "Resume.pdf",
            None,
            Some(8),
        )
        .with_markdown_path(markdown_path.to_string_lossy());
        doc.id = doc_id;

        let mut meeting = MeetingRecord::new(Some("Cleanup".into()));
        meeting.owner_account_id = Some("acct-cleanup".into());
        let restored_id = Uuid::new_v4();
        let restored_preview_dir = account_scoped_restored_dir(
            &root,
            CLOUD_RESTORED_CONTEXT_DIR,
            "acct-cleanup",
            meeting.id,
        )
        .unwrap();
        let restored_object_dir = account_scoped_restored_dir(
            &root,
            CLOUD_RESTORED_OBJECTS_DIR,
            "acct-cleanup",
            meeting.id,
        )
        .unwrap();
        std::fs::create_dir_all(&restored_preview_dir).unwrap();
        std::fs::create_dir_all(&restored_object_dir).unwrap();
        let restored_preview = restored_preview_dir.join(format!("{restored_id}.md"));
        let restored_object = restored_object_dir.join(format!("{restored_id}-Resume.pdf"));
        std::fs::write(&restored_preview, b"private preview").unwrap();
        std::fs::write(&restored_object, b"private object").unwrap();
        let mut restored = ContextArtifact::new(
            ContextKind::Document,
            restored_object.to_string_lossy(),
            "Resume.pdf",
            None,
            Some(14),
        )
        .with_markdown_path(restored_preview.to_string_lossy());
        restored.id = restored_id;
        meeting.context.push(image);
        meeting.context.push(doc);
        meeting.context.push(restored);

        remove_bluey_owned_context_files(&root, &meeting);

        assert!(!image_path.exists());
        assert!(!markdown_path.exists());
        assert!(!restored_preview.exists());
        assert!(!restored_object.exists());
        assert!(original_path.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cloud_bundle_restores_meeting_with_context_preview() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-hydrate-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let session_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();

        let bundle = CloudSessionBundle {
            session: SyncSessionRecord {
                session_id: session_id.to_string(),
                title: "Restored interview".into(),
                status: "archived".into(),
                created_at_ms: 10,
                updated_at_ms: 20,
                last_active_at_ms: Some(20),
                answer_style: Some("concise".into()),
                metadata: json!({ "summary": "A restored session summary" }),
                deleted_at_ms: None,
            },
            transcript_segments: vec![SyncTranscriptSegment {
                segment_id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                speaker: "user".into(),
                source: "microphone".into(),
                text: "Tell me about a hard deadline.".into(),
                start_ms: None,
                end_ms: None,
                ts_ms: 11,
                is_final: true,
                deleted_at_ms: None,
                metadata: json!({}),
            }],
            cue_responses: vec![SyncCueResponseRecord {
                response_id: format!("turn-{turn_id}"),
                session_id: session_id.to_string(),
                kind: "answer".into(),
                text: "Use the DAVD dashboard story.".into(),
                source_text: Some("Tell me about a hard deadline.".into()),
                ts_ms: 12,
                provider: Some("bluey_managed".into()),
                model: Some("balanced".into()),
                lane: None,
                task_type: None,
                cost_cents: None,
                balance_cents_after: None,
                cost_label: None,
                artifact_type: None,
                artifact_body: None,
                artifact_confidence: None,
                deleted_at_ms: None,
                metadata: json!({ "attachment_ids": [artifact_id.to_string()] }),
            }],
            context_artifacts: vec![SyncContextArtifactRecord {
                artifact_id: artifact_id.to_string(),
                session_id: session_id.to_string(),
                kind: "document".into(),
                title: "Resume.pdf".into(),
                note: None,
                source_uri: Some("/old/device/Resume.pdf".into()),
                content_hash: None,
                text_preview: Some("NBCUniversal DAVD dashboard experience".into()),
                created_at_ms: 13,
                updated_at_ms: 13,
                deleted_at_ms: None,
                metadata: json!({
                    "size_bytes": 1234,
                    "processing_status": "ready",
                    "answer_context_role": "candidate_resume"
                }),
            }],
            rag_chunks: Vec::new(),
            child_tombstones: Vec::new(),
        };

        let (meeting, attachment_retries, _) =
            meeting_from_cloud_bundle(&root, None, "acct-restored-session", bundle, &|| Ok(()))
                .await
                .expect("restore meeting");
        assert_eq!(attachment_retries, 0);
        assert_eq!(meeting.id, session_id);
        assert_eq!(meeting.title, "Restored interview");
        assert_eq!(meeting.transcript.len(), 1);
        assert_eq!(meeting.context.len(), 1);
        assert_eq!(
            meeting.context[0].kind.to_string(),
            ContextKind::Document.to_string()
        );
        assert_eq!(
            meeting.context[0].processing_status,
            ContextProcessingStatus::Ready
        );
        assert_eq!(
            meeting.context[0].answer_context_role,
            AnswerContextRole::CandidateResume
        );
        assert!(meeting.context[0]
            .markdown_path
            .as_ref()
            .is_some_and(|path| std::path::Path::new(path).exists()));
        assert_eq!(meeting.conversation.len(), 1);
        assert_eq!(meeting.conversation[0].attachment_ids, vec![artifact_id]);

        let _ = std::fs::remove_dir_all(root);
    }
}
