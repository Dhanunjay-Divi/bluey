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
    CloudClient, CloudSessionBundle, SessionAuditBundleResponse, SyncBatchRequest,
    SyncBatchResponse, SyncContextArtifactRecord, SyncCounts, SyncCueResponseRecord,
    SyncRagChunkRecord, SyncSessionRecord, SyncTranscriptSegment,
};
use cue_core::{
    short_session_code, CardArtifactType, ContextArtifact, ContextKind, ContextProcessingStatus,
    ConversationMemory, ConversationTurn, CueCardArtifact, MeetingDiagnostics, MeetingRecord,
    Speaker, TranscriptSegment,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
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
const CLOUD_SYNC_STATE_DIR: &str = "cloud-sync-state";
const CLOUD_DELETE_OUTBOX_DIR: &str = "cloud-delete-outbox";
const CLOUD_SYNC_STATE_SCHEMA_VERSION: u32 = 1;
const CLOUD_DELETE_OUTBOX_SCHEMA_VERSION: u32 = 2;
const LEGACY_CLOUD_DELETE_OUTBOX_SCHEMA_VERSION: u32 = 1;
const CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION: u32 = 1;
const MAX_CLOUD_CONVERSATION_MEMORY_EPOCHS: usize = 8;
const MAX_CLOUD_CONVERSATION_MEMORY_EPOCH_CHARS: usize = 12_000;
const MAX_CLOUD_CONVERSATION_MEMORY_TIMESTAMP_CHARS: usize = 128;

#[derive(Debug, Clone)]
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
    remote_session_id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    account_id: Option<String>,
    device_id: Option<String>,
    generated_at_ms: i64,
    manifest: Value,
    events: Vec<Value>,
    questions: Vec<Value>,
    responses: Vec<Value>,
    transcript: Vec<Value>,
    context: Vec<Value>,
    screen: Vec<Value>,
    artifacts: Vec<Value>,
    costs: Vec<Value>,
    attachments: Vec<Value>,
}

#[derive(Debug, Clone)]
struct BuiltAuditBundle {
    bundle: SessionAuditBundle,
    bytes: Vec<u8>,
    local_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LocalSyncSummary {
    pub accepted: SyncCounts,
    pub batches: usize,
    pub server_time_ms: Option<i64>,
}

pub fn append_session_audit_event(
    data_dir: &Path,
    meeting: &MeetingRecord,
    kind: &str,
    payload: Value,
) -> Result<()> {
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session_id = meeting.id.to_string();
    let session_code = short_session_code(meeting.id);
    let device_id = std::env::var("BLUEY_DEVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let event_dir = session_audit_event_dir(data_dir, meeting.id);
    cue_core::app_paths::create_private_dir(&event_dir)?;
    let event_path = event_dir.join("events.jsonl");
    let mut event_state = load_audit_event_log_state(&event_path)?;
    let sequence = event_state.last_sequence.saturating_add(1);
    let event_id = format!(
        "{session_code}-ui-{sequence:08}-{}",
        Uuid::new_v4().simple()
    );
    let mut record = json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "event_id": event_id,
        "session_id": session_id,
        "session_code": session_code,
        "account_id": meeting.owner_account_id.as_deref(),
        "device_id": device_id.as_deref(),
        "sequence": sequence,
        "kind": kind,
        "created_at_ms": current_epoch_ms(),
        "source": "desktop_ui",
        "payload": metadata_only_audit_payload(payload),
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
    let incoming_bytes = encoded.len() as u64;
    if event_state.record_count.saturating_add(1) > MAX_AUDIT_EVENT_RECORDS
        || event_state.bytes.saturating_add(incoming_bytes) > MAX_AUDIT_EVENT_LOG_BYTES
    {
        event_state = compact_audit_event_log(&event_path, incoming_bytes)?;
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
    file.write_all(&encoded)
        .with_context(|| format!("write {}", event_path.display()))?;
    file.sync_data()
        .with_context(|| format!("sync {}", event_path.display()))?;
    drop(file);
    event_state.last_sequence = sequence;
    event_state.record_count = event_state.record_count.saturating_add(1);
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
fn metadata_only_audit_payload(payload: Value) -> Value {
    let Value::Object(payload) = payload else {
        return json!({
            "content_policy": "metadata_only",
            "payload_redacted": true,
        });
    };

    let mut metadata = Map::new();
    let mut redacted_fields = Vec::new();
    for (key, value) in payload {
        if audit_metadata_key_is_safe(&key) {
            metadata.insert(key, value);
            continue;
        }

        redacted_fields.push(key.clone());
        match value {
            Value::String(value) => {
                metadata.insert(
                    format!("{key}_chars"),
                    Value::from(value.chars().count() as u64),
                );
            }
            Value::Array(values) => {
                metadata.insert(format!("{key}_count"), Value::from(values.len() as u64));
            }
            Value::Object(values) => {
                metadata.insert(
                    format!("{key}_field_count"),
                    Value::from(values.len() as u64),
                );
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
    metadata.insert(
        "content_policy".into(),
        Value::String("metadata_only".into()),
    );
    if !redacted_fields.is_empty() {
        metadata.insert("payload_redacted".into(), Value::Bool(true));
        metadata.insert(
            "redacted_fields".into(),
            Value::Array(redacted_fields.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(metadata)
}

fn audit_metadata_key_is_safe(key: &str) -> bool {
    matches!(
        key,
        "schema_version"
            | "sequence"
            | "generation"
            | "provider"
            | "model"
            | "route"
            | "route_primary"
            | "task_type"
            | "question_intent"
            | "artifact_type"
            | "processing_status"
            | "speaker"
            | "status"
            | "state"
            | "kind"
            | "source"
            | "error_category"
            | "reason_code"
            | "http_status"
            | "stream"
            | "streaming"
            | "is_final"
            | "retryable"
            | "terminal"
            | "success"
            | "context_was_empty"
            | "cost_cents"
            | "balance_cents_after"
            | "input_tokens"
            | "output_tokens"
            | "artifact_confidence"
            | "audio_chunk_storage"
            | "content_policy"
            | "listen_runs"
            | "stt_parse_errors"
            | "stt_provider_errors"
            | "audio_start_errors"
            | "audio_source_errors"
            | "last_stt_provider"
            | "last_error_kind"
    ) || key.ends_with("_id")
        || key.ends_with("_ids")
        || key.ends_with("_count")
        || key.ends_with("_chars")
        || key.ends_with("_bytes")
        || key.ends_with("_ms")
        || key.ends_with("_pct")
        || key.starts_with("is_")
        || key.starts_with("has_")
        || key.starts_with("had_")
}

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
}

impl CloudHydrationSummary {
    pub fn total_sessions(&self) -> usize {
        self.restored_sessions + self.skipped_sessions + self.purged_deleted_sessions
    }
}

pub async fn sync_local_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
) -> Result<LocalSyncSummary> {
    reconcile_prepared_cloud_session_deletes(data_dir, store, owner_account_id)
        .context("reconcile interrupted local session deletions")?;
    flush_pending_cloud_session_deletes(data_dir, client, owner_account_id).await;
    let meetings = store
        .all_meetings()
        .context("failed to load local sessions")?
        .into_iter()
        .filter(|meeting| meeting_belongs_to_owner(meeting, owner_account_id))
        .collect::<Vec<_>>();
    if meetings.is_empty() {
        return Ok(LocalSyncSummary::empty());
    }

    let response_map = load_local_responses(data_dir, &meetings);
    let sync_states = load_cloud_sync_states(data_dir, &meetings);
    for parent_batch in build_object_parent_batches(&meetings, &response_map, &sync_states) {
        client
            .sync_batch(&parent_batch)
            .await
            .context("reserve cloud parent sessions before object upload")?;
    }
    let uploaded_objects = upload_context_objects(data_dir, &meetings, &sync_states, client).await;
    let batches =
        build_sync_batches_with_states(&meetings, &response_map, &uploaded_objects, &sync_states);
    if batches.is_empty() {
        return Ok(LocalSyncSummary::empty());
    }
    let uploaded_child_states = uploaded_child_states_by_session(&batches);

    let mut summary = LocalSyncSummary::empty();
    for batch in batches {
        let response = client
            .sync_batch(&batch)
            .await
            .context("cloud sync batch")?;
        summary.add_response(response);
    }
    for meeting in &meetings {
        let responses = response_map.get(&meeting.id.to_string()).map(Vec::as_slice);
        let previous = sync_states.get(&meeting.id);
        let wire_session_id = wire_session_id(meeting, previous);
        let state = cloud_sync_state_after_upload(
            meeting,
            responses,
            &uploaded_objects,
            previous,
            uploaded_child_states.get(&wire_session_id),
        );
        write_cloud_sync_state(data_dir, meeting.id, &state)?;
    }
    if support_diagnostic_upload_enabled() {
        sync_session_audit_bundles(data_dir, &meetings, &response_map, client, owner_account_id)
            .await;
    } else {
        // Diagnostic bundles are a separate support-data surface. Ordinary
        // session sync must never imply consent to upload them.
        prune_local_audit_storage(data_dir);
    }
    Ok(summary)
}

fn support_diagnostic_upload_enabled() -> bool {
    std::env::var(SUPPORT_DIAGNOSTIC_UPLOAD_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn prepare_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<CloudSessionDeleteDisposition> {
    let remote_session_id = load_cloud_sync_state(data_dir, local_session_id)
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
    };
    write_pending_cloud_delete(data_dir, &pending)?;
    Ok(CloudSessionDeleteDisposition::Queued)
}

pub fn commit_prepared_cloud_session_delete(
    data_dir: &Path,
    local_session_id: Uuid,
    owner_account_id: &str,
) -> Result<CloudSessionDeleteDisposition> {
    let path = cloud_delete_outbox_path(data_dir, local_session_id);
    let mut pending = read_cloud_delete_intent(&path)?.with_context(|| {
        format!("prepared cloud deletion for session {local_session_id} missing")
    })?;
    validate_cloud_delete_intent(&pending, local_session_id, owner_account_id)?;
    if pending.state == CloudSessionDeleteState::Committed {
        return Ok(CloudSessionDeleteDisposition::Queued);
    }
    pending.state = CloudSessionDeleteState::Committed;
    pending.committed_at_ms = Some(current_epoch_ms());
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
    let dir = data_dir.join(CLOUD_DELETE_OUTBOX_DIR);
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
    let path = cloud_delete_outbox_path(data_dir, local_session_id);
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
) -> CloudSessionDeleteDisposition {
    flush_queued_cloud_session_delete_with(data_dir, local_session_id, client, owner_account_id)
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
) -> CloudSessionDeleteDisposition
where
    C: CloudSessionDeleteClient + Sync + ?Sized,
{
    let path = cloud_delete_outbox_path(data_dir, local_session_id);
    let Some(pending) = read_cloud_delete_intent(&path).ok().flatten() else {
        return CloudSessionDeleteDisposition::NotPreviouslyUploaded;
    };
    if validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_err() {
        return CloudSessionDeleteDisposition::NotPreviouslyUploaded;
    }
    if pending.state != CloudSessionDeleteState::Committed {
        return CloudSessionDeleteDisposition::Queued;
    }
    match client
        .delete_cloud_session(&pending.remote_session_id)
        .await
    {
        Ok(_) => {
            remove_cloud_delete_provenance(data_dir, local_session_id);
            CloudSessionDeleteDisposition::Confirmed
        }
        Err(error) => {
            warn!(
                session_id = %local_session_id,
                error_category = %cloud_delete_error_category(&error),
                "cloud session deletion remains queued"
            );
            CloudSessionDeleteDisposition::Queued
        }
    }
}

pub async fn flush_pending_cloud_session_deletes(
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
) {
    flush_pending_cloud_session_deletes_with(data_dir, client, owner_account_id).await;
}

async fn flush_pending_cloud_session_deletes_with<C>(
    data_dir: &Path,
    client: &C,
    owner_account_id: Option<&str>,
) where
    C: CloudSessionDeleteClient + Sync + ?Sized,
{
    let Some(owner_account_id) = owner_account_id
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
    else {
        return;
    };
    let dir = data_dir.join(CLOUD_DELETE_OUTBOX_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten().take(256) {
        let path = entry.path();
        let Some(local_session_id) = cloud_delete_outbox_session_id(&path) else {
            continue;
        };
        let Some(pending) = read_cloud_delete_intent(&path).ok().flatten() else {
            continue;
        };
        if validate_cloud_delete_intent(&pending, local_session_id, owner_account_id).is_err()
            || pending.state != CloudSessionDeleteState::Committed
        {
            continue;
        }
        match client
            .delete_cloud_session(&pending.remote_session_id)
            .await
        {
            Ok(_) => {
                remove_cloud_delete_provenance(data_dir, local_session_id);
            }
            Err(error) => {
                debug!(
                    session_id = %local_session_id,
                    error_category = %cloud_delete_error_category(&error),
                    "pending cloud session deletion remains queued"
                );
            }
        }
    }
}

fn cloud_delete_outbox_session_id(path: &Path) -> Option<Uuid> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
        return None;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Uuid::parse_str(stem).ok())
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

pub async fn hydrate_missing_cloud_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
    owner_account_id: Option<&str>,
    limit: i64,
) -> Result<CloudHydrationSummary> {
    let response = client
        .list_cloud_sessions(Some(limit.clamp(1, 200)))
        .await
        .context("list cloud sessions")?;
    let mut summary = CloudHydrationSummary::default();
    for deleted in response.deleted_sessions {
        let session_id = local_uuid_for_cloud_id("session", "account-session", &deleted.session_id);
        let Some(local_meeting) = store.load_by_id(session_id)? else {
            continue;
        };
        if !meeting_should_follow_cloud_delete(&local_meeting, owner_account_id) {
            summary.skipped_sessions += 1;
            continue;
        }
        remove_bluey_owned_context_files(data_dir, &local_meeting);
        if store.delete(session_id)? {
            let state_path = cloud_sync_state_path(data_dir, session_id);
            if let Err(error) = fs::remove_file(&state_path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    debug!(
                        path = %state_path.display(),
                        error = %error,
                        "cloud-delete cleanup could not remove sync state"
                    );
                }
            }
            summary.purged_deleted_sessions += 1;
        }
    }

    if response.sessions.is_empty() {
        return Ok(summary);
    }

    for session in response.sessions {
        let session_id = local_uuid_for_cloud_id("session", "account-session", &session.session_id);
        let existing = store.load_by_id(session_id)?;
        if existing
            .as_ref()
            .is_some_and(|meeting| !meeting_belongs_to_owner(meeting, owner_account_id))
        {
            summary.skipped_sessions += 1;
            continue;
        }

        let bundle = client
            .load_cloud_session(&session.session_id)
            .await
            .with_context(|| format!("load cloud session {}", session.session_id))?;
        if bundle.session.session_id != session.session_id {
            anyhow::bail!(
                "cloud session list/bundle identity mismatch: expected {}, got {}",
                session.session_id,
                bundle.session.session_id
            );
        }
        let mut cloud_meeting = meeting_from_cloud_bundle(data_dir, Some(client), bundle).await?;
        cloud_meeting.owner_account_id = owner_account_id.map(ToString::to_string);
        if !meeting_has_syncable_content(&cloud_meeting, None) {
            summary.skipped_sessions += 1;
            continue;
        }

        if let Some(existing) = existing {
            let active = store
                .load_active()?
                .is_some_and(|meeting| meeting.id == existing.id);
            let (meeting, changed) = reconcile_cloud_meeting(existing, cloud_meeting)?;
            if !changed {
                summary.skipped_sessions += 1;
                continue;
            }
            if active {
                store.save_active(&meeting)?;
            } else {
                store.save_archived(&meeting)?;
            }
        } else {
            store.save_archived(&cloud_meeting)?;
        }
        summary.restored_sessions += 1;
    }
    Ok(summary)
}

fn reconcile_cloud_meeting(
    mut local: MeetingRecord,
    cloud: MeetingRecord,
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

fn merge_missing_context_fields(local: &mut ContextArtifact, cloud: ContextArtifact) {
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
    artifact_id: Uuid,
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
        warn!(
            artifact_id = %artifact_id,
            path = %candidate.display(),
            allowed = %allowed.display(),
            "skipping cloud-delete cleanup outside Bluey context directory"
        );
        return;
    }
    if let Err(error) = fs::remove_file(&candidate) {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(
                artifact_id = %artifact_id,
                path = %candidate.display(),
                kind,
                "failed to remove Bluey-owned context cache file after cloud delete: {error}"
            );
        }
    }
}

fn load_local_responses(
    data_dir: &Path,
    meetings: &[MeetingRecord],
) -> HashMap<String, Vec<crate::llm::CueResponse>> {
    let db_path = data_dir.join("sessions.db");
    let Ok(db) = Database::open(db_path.to_str().unwrap_or("sessions.db")) else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for meeting in meetings {
        let session_id = meeting.id.to_string();
        match db.list_cue_responses(&session_id, 500) {
            Ok(mut responses) => {
                responses.sort_by_key(|response| response.ts_ms);
                out.insert(session_id, responses);
            }
            Err(error) => {
                debug!(
                    session_id = %session_id,
                    error = %error,
                    "cloud sync could not read local cue responses"
                );
            }
        }
    }
    out
}

async fn upload_context_objects(
    data_dir: &Path,
    meetings: &[MeetingRecord],
    sync_states: &HashMap<Uuid, CloudSyncState>,
    client: &CloudClient,
) -> HashMap<Uuid, SyncedObjectMetadata> {
    let mut uploaded = HashMap::new();
    let mut seen = HashSet::new();
    for meeting in meetings {
        let sync_state = sync_states.get(&meeting.id);
        for artifact in &meeting.context {
            if !seen.insert(artifact.id) {
                continue;
            }
            let Some(path) = artifact_object_path(data_dir, artifact) else {
                continue;
            };
            let Ok(metadata) = tokio::fs::metadata(&path).await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if metadata.len() > MAX_OBJECT_UPLOAD_BYTES {
                debug!(
                    artifact_id = %artifact.id,
                    path = %path.display(),
                    size_bytes = metadata.len(),
                    "cloud object sync skipped oversized artifact"
                );
                continue;
            }
            let content_type = content_type_for_path(&path);
            let artifact_id = wire_context_artifact_id(meeting, artifact.id, sync_state);
            let session_id = wire_session_id(meeting, sync_state);
            if Uuid::parse_str(&artifact_id).is_err() {
                // The object endpoint is UUID-keyed. Legacy non-UUID record IDs
                // still round-trip through JSON sync, but their unavailable
                // object bytes are intentionally not attached to a new ID.
                debug!(
                    artifact_id = %artifact_id,
                    "cloud object sync skipped legacy non-UUID artifact id"
                );
                continue;
            }
            match tokio::fs::read(&path).await {
                Ok(bytes) => match client
                    .upload_artifact_object(&artifact_id, &session_id, bytes, &content_type)
                    .await
                {
                    Ok(response) => {
                        uploaded.insert(
                            artifact.id,
                            SyncedObjectMetadata {
                                object_key: response.object_key,
                                size_bytes: response.size_bytes,
                                sha256: response.sha256,
                                content_type: response.content_type,
                                expires_at_ms: response.expires_at_ms,
                            },
                        );
                    }
                    Err(error) => {
                        warn!(
                            artifact_id = %artifact.id,
                            path = %path.display(),
                            error = %error,
                            "cloud object upload failed; continuing with text sync"
                        );
                    }
                },
                Err(error) => {
                    debug!(
                        artifact_id = %artifact.id,
                        path = %path.display(),
                        error = %error,
                        "cloud object sync could not read artifact"
                    );
                }
            }
        }
    }
    uploaded
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

async fn sync_session_audit_bundles(
    data_dir: &Path,
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    client: &CloudClient,
    owner_account_id: Option<&str>,
) {
    prune_local_audit_storage(data_dir);
    for meeting in meetings {
        let session_id = meeting.id.to_string();
        let responses = response_map.get(&session_id);
        if !meeting_has_syncable_content(meeting, responses) {
            continue;
        }
        let response_slice = responses.map(Vec::as_slice).unwrap_or(&[]);
        let built = match build_local_session_audit_bundle(
            data_dir,
            meeting,
            response_slice,
            owner_account_id,
        ) {
            Ok(bundle) => bundle,
            Err(error) => {
                warn!(
                    session_id = %meeting.id,
                    error = %error,
                    "session audit bundle build failed"
                );
                continue;
            }
        };
        if built.bytes.len() > MAX_AUDIT_BUNDLE_BYTES {
            warn!(
                session_id = %meeting.id,
                size_bytes = built.bytes.len(),
                max_bytes = MAX_AUDIT_BUNDLE_BYTES,
                "session audit bundle skipped because it is too large"
            );
            continue;
        }
        if audit_upload_marker_matches(data_dir, meeting, &built.bundle.bundle_id) {
            if let Err(error) = fs::remove_dir_all(&built.local_dir) {
                debug!(
                    session_id = %meeting.id,
                    path = %built.local_dir.display(),
                    error = %error,
                    "already uploaded session audit bundle could not be removed locally"
                );
            }
            continue;
        }
        match client
            .upload_session_audit_bundle(
                &session_id,
                &built.bundle.bundle_id,
                built.bytes.clone(),
                AUDIT_BUNDLE_CONTENT_TYPE,
            )
            .await
        {
            Ok(response) => {
                if let Err(error) = write_audit_upload_marker(data_dir, meeting, &built, &response)
                {
                    debug!(
                        session_id = %meeting.id,
                        error = %error,
                        "session audit upload marker write failed"
                    );
                }
                if let Err(error) = fs::remove_dir_all(&built.local_dir) {
                    warn!(
                        session_id = %meeting.id,
                        path = %built.local_dir.display(),
                        error = %error,
                        "uploaded session audit bundle could not be removed locally"
                    );
                }
                if let Err(error) = remove_session_audit_event_log(data_dir, meeting.id) {
                    debug!(
                        session_id = %meeting.id,
                        error = %error,
                        "uploaded session audit event log could not be removed locally"
                    );
                }
            }
            Err(error) => {
                warn!(
                    session_id = %meeting.id,
                    bundle_id = %built.bundle.bundle_id,
                    error = %error,
                    "session audit bundle upload failed; local retry copy remains bounded"
                );
            }
        }
    }
    prune_local_audit_storage(data_dir);
}

fn build_local_session_audit_bundle(
    data_dir: &Path,
    meeting: &MeetingRecord,
    responses: &[crate::llm::CueResponse],
    account_id: Option<&str>,
) -> Result<BuiltAuditBundle> {
    let audit_root = data_dir.join(SESSION_AUDIT_DIR);
    let local_dir = audit_root.join(meeting.id.to_string());
    let audio_dir = local_dir.join("audio");
    cue_core::app_paths::create_private_dir(&local_dir)?;
    cue_core::app_paths::create_private_dir(&audio_dir)?;

    let bundle = assemble_session_audit_bundle(data_dir, meeting, responses, account_id);
    let bytes = serde_json::to_vec_pretty(&bundle).context("serialize session audit bundle")?;
    write_json_file(&local_dir.join("manifest.json"), &bundle.manifest)?;
    write_jsonl_file(&local_dir.join("events.jsonl"), &bundle.events)?;
    write_jsonl_file(&local_dir.join("questions.jsonl"), &bundle.questions)?;
    write_jsonl_file(&local_dir.join("responses.jsonl"), &bundle.responses)?;
    write_jsonl_file(&local_dir.join("transcript.jsonl"), &bundle.transcript)?;
    write_jsonl_file(&local_dir.join("context.jsonl"), &bundle.context)?;
    write_jsonl_file(&local_dir.join("screen.jsonl"), &bundle.screen)?;
    write_jsonl_file(&local_dir.join("artifacts.jsonl"), &bundle.artifacts)?;
    write_jsonl_file(&local_dir.join("costs.jsonl"), &bundle.costs)?;
    write_jsonl_file(&local_dir.join("attachments.jsonl"), &bundle.attachments)?;
    write_jsonl_file(
        &audio_dir.join("audio.jsonl"),
        &[json!({
            "schema_version": AUDIT_SCHEMA_VERSION,
            "session_id": meeting.id.to_string(),
            "session_code": short_session_code(meeting.id),
            "kind": "audio_capture_manifest",
            "created_at_ms": current_epoch_ms(),
            "audio_chunk_storage": "not_collected",
            "content_policy": "metadata_only",
        })],
    )?;
    write_json_file(&local_dir.join("bundle.json"), &bundle)?;

    Ok(BuiltAuditBundle {
        bundle,
        bytes,
        local_dir,
    })
}

fn assemble_session_audit_bundle(
    data_dir: &Path,
    meeting: &MeetingRecord,
    responses: &[crate::llm::CueResponse],
    account_id: Option<&str>,
) -> SessionAuditBundle {
    let session_id = meeting.id.to_string();
    let session_code = short_session_code(meeting.id);
    let generated_at_ms = current_epoch_ms();
    let event_log_fingerprint = audit_event_log_fingerprint(data_dir, meeting.id);
    let updated_at = updated_at_ms(meeting, Some(responses))
        .max(
            event_log_fingerprint
                .map(|(modified_ms, _)| modified_ms)
                .unwrap_or_default(),
        )
        .max(
            responses
                .iter()
                .map(|response| response.ts_ms as i64)
                .max()
                .unwrap_or_default(),
        );
    let event_log_size = event_log_fingerprint
        .map(|(_, size_bytes)| size_bytes)
        .unwrap_or(0);
    let bundle_id = format!("audit-{session_code}-{updated_at}-{event_log_size}");
    let device_id = std::env::var("BLUEY_DEVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());

    let mut events = Vec::new();
    let mut questions = Vec::new();
    let mut response_records = Vec::new();
    let mut transcript = Vec::new();
    let mut context = Vec::new();
    let mut screen = Vec::new();
    let mut artifacts = Vec::new();
    let mut costs = Vec::new();
    let mut attachments = Vec::new();
    let mut sequence = 0u64;
    let mut seen_questions = HashSet::new();

    push_audit_record(
        &mut events,
        &session_id,
        &session_code,
        account_id,
        &device_id,
        &mut sequence,
        "session",
        json!({
            "started_at_ms": parse_ms(&meeting.started_at),
            "ended_at_ms": meeting.ended_at.as_deref().map(parse_ms),
            "updated_at_ms": updated_at,
            "listen_runs": meeting.diagnostics.listen_runs,
            "stt_parse_errors": meeting.diagnostics.stt_parse_errors,
            "stt_provider_errors": meeting.diagnostics.stt_provider_errors,
            "audio_start_errors": meeting.diagnostics.audio_start_errors,
            "audio_source_errors": meeting.diagnostics.audio_source_errors,
            "last_audio_session_id": meeting.diagnostics.last_audio_session_id,
            "last_stt_provider": meeting.diagnostics.last_stt_provider,
            "last_error_kind": meeting.diagnostics.last_error_kind,
            "last_error_at_ms": meeting.diagnostics.last_error_at.as_deref().map(parse_ms),
            "title_chars": meeting.title.chars().count(),
            "summary_chars": meeting.summary.as_deref().map(|value| value.chars().count()),
            "answer_style_chars": meeting.answer_instructions.as_deref().map(|value| value.chars().count()),
        }),
    );

    for segment in &meeting.transcript {
        let payload = json!({
            "segment_id": segment.id.to_string(),
            "speaker": segment.speaker.to_string(),
            "text": truncate_chars(&segment.text, MAX_TEXT_PREVIEW_CHARS),
            "created_at_ms": parse_ms(&segment.created_at),
            "is_final": segment.is_final,
        });
        push_audit_record(
            &mut transcript,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "transcript_segment",
            payload.clone(),
        );
        push_audit_record(
            &mut events,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "transcript_segment",
            json!({ "segment_id": segment.id.to_string(), "is_final": segment.is_final }),
        );
    }

    for artifact in &meeting.context {
        let payload = json!({
            "artifact_id": artifact.id.to_string(),
            "kind": artifact.kind.to_string(),
            "title": artifact.title,
            "note": artifact.note,
            "path": artifact.path,
            "size_bytes": artifact.size_bytes,
            "text_preview": artifact.text_preview.as_deref().map(|text| truncate_chars(text, MAX_TEXT_PREVIEW_CHARS)),
            "markdown_path": artifact.markdown_path,
            "processing_status": artifact.processing_status.to_string(),
            "processing_error": artifact.processing_error,
            "created_at_ms": parse_ms(&artifact.created_at),
        });
        push_audit_record(
            &mut context,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "context_artifact",
            payload.clone(),
        );
        push_audit_record(
            &mut attachments,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "attachment",
            payload.clone(),
        );
        if matches!(artifact.kind, ContextKind::Image | ContextKind::Diagram)
            || artifact.title.to_ascii_lowercase().contains("screen")
        {
            push_audit_record(
                &mut screen,
                &session_id,
                &session_code,
                account_id,
                &device_id,
                &mut sequence,
                "screen_context",
                payload.clone(),
            );
        }
        push_audit_record(
            &mut events,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "context_artifact",
            json!({ "artifact_id": artifact.id.to_string(), "kind": artifact.kind.to_string() }),
        );
    }

    for response in responses {
        if let Some(question) = response
            .source_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            let key = format!("{}:{}", response.id, question.trim());
            if seen_questions.insert(key) {
                push_audit_record(
                    &mut questions,
                    &session_id,
                    &session_code,
                    account_id,
                    &device_id,
                    &mut sequence,
                    "question",
                    json!({
                        "response_id": response.id,
                        "text": truncate_chars(question, MAX_TEXT_PREVIEW_CHARS),
                        "created_at_ms": response.ts_ms as i64,
                        "source": "cue_response",
                    }),
                );
            }
        }
        push_audit_record(
            &mut response_records,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "response",
            json!({
                "response_id": response.id,
                "kind": response.kind,
                "text": truncate_chars(&response.text, MAX_RESPONSE_CHARS),
                "created_at_ms": response.ts_ms as i64,
                "provider": response.provider,
                "model": response.model,
                "cost_label": response.cost_label,
                "artifact_type": response.artifact_type,
                "artifact_confidence": response.artifact_confidence,
            }),
        );
        push_response_cost_record(
            &mut costs,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            response,
        );
        if response.artifact_type.is_some() || response.artifact_body.is_some() {
            push_audit_record(
                &mut artifacts,
                &session_id,
                &session_code,
                account_id,
                &device_id,
                &mut sequence,
                "response_artifact",
                json!({
                    "response_id": response.id,
                    "artifact_type": response.artifact_type,
                    "artifact_body": response.artifact_body.as_deref().map(|body| truncate_chars(body, MAX_RESPONSE_CHARS)),
                    "artifact_confidence": response.artifact_confidence,
                }),
            );
        }
    }

    for turn in &meeting.conversation {
        let key = format!("turn:{}:{}", turn.id, turn.question.trim());
        if !turn.question.trim().is_empty() && seen_questions.insert(key) {
            push_audit_record(
                &mut questions,
                &session_id,
                &session_code,
                account_id,
                &device_id,
                &mut sequence,
                "question",
                json!({
                    "turn_id": turn.id.to_string(),
                    "text": truncate_chars(&turn.question, MAX_TEXT_PREVIEW_CHARS),
                    "created_at_ms": parse_ms(&turn.created_at),
                    "source": turn.source,
                    "attachment_ids": turn.attachment_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                }),
            );
        }
        push_audit_record(
            &mut response_records,
            &session_id,
            &session_code,
            account_id,
            &device_id,
            &mut sequence,
            "response",
            json!({
                "turn_id": turn.id.to_string(),
                "text": truncate_chars(&turn.answer, MAX_RESPONSE_CHARS),
                "created_at_ms": parse_ms(&turn.created_at),
                "provider": turn.provider,
                "source": turn.source,
                "attachment_ids": turn.attachment_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                "artifact_type": turn.artifact.as_ref().map(|artifact| cloud_artifact_type_value(artifact.artifact_type)),
            }),
        );
        if let Some(artifact) = &turn.artifact {
            push_audit_record(
                &mut artifacts,
                &session_id,
                &session_code,
                account_id,
                &device_id,
                &mut sequence,
                "turn_artifact",
                json!({
                    "turn_id": turn.id.to_string(),
                    "artifact_type": cloud_artifact_type_value(artifact.artifact_type),
                    "title": artifact.title,
                    "body": truncate_chars(&artifact.body, MAX_RESPONSE_CHARS),
                    "confidence": artifact.confidence,
                }),
            );
        }
    }

    let raw_ui_events = read_session_audit_events(data_dir, meeting.id);
    let raw_ui_event_count = raw_ui_events.len();
    events.extend(raw_ui_events);

    let manifest = json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "bundle_id": bundle_id,
        "session_id": session_id,
        "session_code": session_code,
        "account_id": account_id,
        "device_id": device_id,
        "generated_at_ms": generated_at_ms,
        "updated_at_ms": updated_at,
        "record_counts": {
            "events": events.len(),
            "questions": questions.len(),
            "responses": response_records.len(),
            "transcript": transcript.len(),
            "context": context.len(),
            "screen": screen.len(),
            "artifacts": artifacts.len(),
            "costs": costs.len(),
            "attachments": attachments.len(),
            "raw_ui_events": raw_ui_event_count,
        },
        "local_retention": {
            "uploaded_session_dirs_removed": true,
            "failed_upload_dirs_retention_days": audit_local_retention_days(),
            "failed_upload_root_max_bytes": audit_local_max_bytes(),
        },
    });

    SessionAuditBundle {
        schema_version: AUDIT_SCHEMA_VERSION,
        bundle_id,
        session_id,
        session_code,
        account_id: account_id.map(ToString::to_string),
        device_id,
        generated_at_ms,
        manifest,
        events,
        questions,
        responses: response_records,
        transcript,
        context,
        screen,
        artifacts,
        costs,
        attachments,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_audit_record(
    records: &mut Vec<Value>,
    session_id: &str,
    session_code: &str,
    account_id: Option<&str>,
    device_id: &Option<String>,
    sequence: &mut u64,
    kind: &str,
    payload: Value,
) {
    *sequence += 1;
    records.push(json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "event_id": format!("{session_code}-{:08}", *sequence),
        "session_id": session_id,
        "session_code": session_code,
        "account_id": account_id,
        "device_id": device_id.as_deref(),
        "sequence": *sequence,
        "kind": kind,
        "created_at_ms": current_epoch_ms(),
        "source": "desktop_sync",
        "payload": metadata_only_audit_payload(payload),
    }));
}

#[allow(clippy::too_many_arguments)]
fn push_response_cost_record(
    records: &mut Vec<Value>,
    session_id: &str,
    session_code: &str,
    account_id: Option<&str>,
    device_id: &Option<String>,
    sequence: &mut u64,
    response: &crate::llm::CueResponse,
) {
    if response.cost_cents.is_none()
        && response.balance_cents_after.is_none()
        && response.input_tokens.is_none()
        && response.output_tokens.is_none()
    {
        return;
    }
    push_audit_record(
        records,
        session_id,
        session_code,
        account_id,
        device_id,
        sequence,
        "cost",
        json!({
            "response_id": response.id,
            "provider": response.provider,
            "model": response.model,
            "input_tokens": response.input_tokens,
            "output_tokens": response.output_tokens,
            "cost_cents": response.cost_cents,
            "balance_cents_after": response.balance_cents_after,
            "cost_label": response.cost_label,
            "created_at_ms": response.ts_ms as i64,
        }),
    );
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

fn session_audit_event_dir(data_dir: &Path, session_id: Uuid) -> PathBuf {
    data_dir
        .join(SESSION_AUDIT_EVENTS_DIR)
        .join(session_id.to_string())
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
    let state = compact_audit_event_log(event_path, 0)?;
    write_audit_event_log_state(event_path, &state)?;
    Ok(state)
}

fn compact_audit_event_log(event_path: &Path, reserved_bytes: u64) -> Result<AuditEventLogState> {
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
            while retained.len() as u64 > MAX_AUDIT_EVENT_RECORDS.saturating_sub(1)
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

fn read_session_audit_events(data_dir: &Path, session_id: Uuid) -> Vec<Value> {
    let event_path = session_audit_event_dir(data_dir, session_id).join("events.jsonl");
    let _append_guard = audit_event_append_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !event_path.exists() || load_audit_event_log_state(&event_path).is_err() {
        return Vec::new();
    }
    let Ok(file) = fs::File::open(&event_path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .collect()
}

fn audit_event_log_fingerprint(data_dir: &Path, session_id: Uuid) -> Option<(i64, u64)> {
    let event_path = session_audit_event_dir(data_dir, session_id).join("events.jsonl");
    let metadata = fs::metadata(&event_path).ok()?;
    Some((metadata_modified_ms(&metadata), metadata.len()))
}

fn remove_session_audit_event_log(data_dir: &Path, session_id: Uuid) -> Result<()> {
    let event_dir = session_audit_event_dir(data_dir, session_id);
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
pub fn purge_session_audit_state(data_dir: &Path, session_id: Uuid) -> Result<()> {
    let local_bundle_dir = data_dir
        .join(SESSION_AUDIT_DIR)
        .join(session_id.to_string());
    let event_dir = session_audit_event_dir(data_dir, session_id);
    let upload_marker = data_dir
        .join(SESSION_AUDIT_UPLOADED_DIR)
        .join(format!("{session_id}.json"));

    remove_audit_path_if_present(&local_bundle_dir, true)?;
    remove_audit_path_if_present(&event_dir, true)?;
    remove_audit_path_if_present(&upload_marker, false)?;
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
    let marker_path = data_dir
        .join(SESSION_AUDIT_UPLOADED_DIR)
        .join(format!("{}.json", meeting.id));
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
    let marker_dir = data_dir.join(SESSION_AUDIT_UPLOADED_DIR);
    cue_core::app_paths::create_private_dir(&marker_dir)?;
    let marker_path = marker_dir.join(format!("{}.json", meeting.id));
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
                    path = %entry.path.display(),
                    error = %error,
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
                path = %entry.path.display(),
                error = %error,
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
    read_dir
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = path.file_name().and_then(|value| value.to_str())?;
            if name.starts_with('.') {
                return None;
            }
            let metadata = fs::metadata(&path).ok()?;
            Some(AuditDirEntry {
                path,
                modified_ms: metadata_modified_ms(&metadata),
                size_bytes: dir_size_bytes(entry.path().as_path()),
            })
        })
        .collect()
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

fn cloud_sync_state_path(data_dir: &Path, session_id: Uuid) -> PathBuf {
    data_dir
        .join(CLOUD_SYNC_STATE_DIR)
        .join(format!("{session_id}.json"))
}

fn cloud_sync_state_backup_path(data_dir: &Path, session_id: Uuid) -> PathBuf {
    data_dir
        .join(CLOUD_SYNC_STATE_DIR)
        .join(format!("{session_id}.json.bak"))
}

fn cloud_delete_outbox_path(data_dir: &Path, session_id: Uuid) -> PathBuf {
    data_dir
        .join(CLOUD_DELETE_OUTBOX_DIR)
        .join(format!("{session_id}.json"))
}

fn write_pending_cloud_delete(data_dir: &Path, pending: &PendingCloudSessionDelete) -> Result<()> {
    let dir = data_dir.join(CLOUD_DELETE_OUTBOX_DIR);
    cue_core::app_paths::create_private_dir(&dir)?;
    let bytes =
        serde_json::to_vec_pretty(pending).context("serialize pending cloud session deletion")?;
    atomic_write_private_file(
        &cloud_delete_outbox_path(data_dir, pending.local_session_id),
        &bytes,
    )
}

fn remove_cloud_delete_provenance(data_dir: &Path, session_id: Uuid) {
    for path in [
        cloud_delete_outbox_path(data_dir, session_id),
        cloud_sync_state_path(data_dir, session_id),
        cloud_sync_state_backup_path(data_dir, session_id),
    ] {
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                debug!(
                    path = %path.display(),
                    error = %error,
                    "could not remove completed cloud deletion provenance"
                );
            }
        }
    }
}

fn load_cloud_sync_state(data_dir: &Path, session_id: Uuid) -> Option<CloudSyncState> {
    let path = cloud_sync_state_path(data_dir, session_id);
    if let Some(state) = read_valid_cloud_sync_state(&path) {
        return Some(state);
    }

    let backup_path = cloud_sync_state_backup_path(data_dir, session_id);
    let recovered = read_valid_cloud_sync_state(&backup_path);
    if recovered.is_some() {
        warn!(
            path = %path.display(),
            backup_path = %backup_path.display(),
            "recovered cloud sync deletion provenance from the last valid backup"
        );
    }
    recovered
}

fn read_valid_cloud_sync_state(path: &Path) -> Option<CloudSyncState> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice::<CloudSyncState>(&bytes) {
        Ok(state)
            if state.schema_version == CLOUD_SYNC_STATE_SCHEMA_VERSION
                && !state.remote_session_id.trim().is_empty() =>
        {
            Some(state)
        }
        Ok(_) => None,
        Err(error) => {
            debug!(
                path = %path.display(),
                error = %error,
                "cloud sync state could not be read"
            );
            None
        }
    }
}

fn load_cloud_sync_states(
    data_dir: &Path,
    meetings: &[MeetingRecord],
) -> HashMap<Uuid, CloudSyncState> {
    meetings
        .iter()
        .filter_map(|meeting| {
            load_cloud_sync_state(data_dir, meeting.id).map(|state| (meeting.id, state))
        })
        .collect()
}

fn write_cloud_sync_state(data_dir: &Path, session_id: Uuid, state: &CloudSyncState) -> Result<()> {
    let dir = data_dir.join(CLOUD_SYNC_STATE_DIR);
    cue_core::app_paths::create_private_dir(&dir)?;
    let path = cloud_sync_state_path(data_dir, session_id);
    let backup_path = cloud_sync_state_backup_path(data_dir, session_id);
    if read_valid_cloud_sync_state(&path).is_some() {
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
    CloudSyncState {
        schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
        remote_session_id: session.session_id,
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

fn artifact_object_path(data_dir: &Path, artifact: &ContextArtifact) -> Option<PathBuf> {
    let restored_preview_dir = data_dir.join("cloud-restored-context");
    let candidates = std::iter::once(Some(artifact.path.as_str()))
        .chain(std::iter::once(artifact.markdown_path.as_deref()));
    for candidate in candidates.flatten() {
        if candidate.trim().is_empty() {
            continue;
        }
        let path = PathBuf::from(candidate);
        if path.starts_with(&restored_preview_dir) {
            continue;
        }
        if path.is_file() {
            return Some(path);
        }
    }
    None
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
                session_id = %meeting.id,
                title = %meeting.title,
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
            maybe_flush(&mut batches, &mut batch, &session);
            batch
                .transcript_segments
                .push(transcript_record(meeting, segment, sync_state));

            if segment.is_final {
                if let Some(chunk) = transcript_rag_chunk(meeting, segment, sync_state) {
                    maybe_flush(&mut batches, &mut batch, &session);
                    batch.rag_chunks.push(chunk);
                }
            }
        }

        for artifact in &meeting.context {
            maybe_flush(&mut batches, &mut batch, &session);
            batch.context_artifacts.push(context_record(
                meeting,
                artifact,
                uploaded_objects,
                sync_state,
            ));
            if let Some(chunk) = context_rag_chunk(meeting, artifact, sync_state) {
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
            maybe_flush(&mut batches, &mut batch, &session);
            batch.rag_chunks.push(SyncRagChunkRecord {
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
            });
        }

        for (index, epoch) in meeting.conversation_memory.epochs.iter().enumerate() {
            let Some(text) = truncate_nonempty(&epoch.summary) else {
                continue;
            };
            maybe_flush(&mut batches, &mut batch, &session);
            batch.rag_chunks.push(SyncRagChunkRecord {
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
            });
        }

        if let Some(instructions) = meeting
            .answer_instructions
            .as_deref()
            .and_then(truncate_nonempty)
        {
            maybe_flush(&mut batches, &mut batch, &session);
            batch.rag_chunks.push(SyncRagChunkRecord {
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
            });
        }

        if let Some(responses) = responses {
            for response in responses {
                let response_record = cue_response_record(meeting, response, sync_state);
                let response_id = response_record.response_id.clone();
                seen_response_ids.insert(response_id.clone());
                maybe_flush(&mut batches, &mut batch, &session);
                batch.cue_responses.push(response_record);
                if let Some(chunk) = response_rag_chunk(meeting, response, &response_id, sync_state)
                {
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
            maybe_flush(&mut batches, &mut batch, &session);
            batch.cue_responses.push(response_record);
            if let Some(chunk) = conversation_rag_chunk(meeting, turn, sync_state) {
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
    bundle: CloudSessionBundle,
) -> Result<MeetingRecord> {
    validate_cloud_bundle_parentage(&bundle)?;
    let CloudSessionBundle {
        session,
        transcript_segments,
        cue_responses,
        context_artifacts,
    } = bundle;
    let remote_session_id = session.session_id.clone();
    let id = local_uuid_for_cloud_id("session", "account-session", &remote_session_id);
    let mut sync_state = CloudSyncState {
        schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
        remote_session_id: remote_session_id.clone(),
        session_metadata: session.metadata.clone(),
        transcript_segments: BTreeMap::new(),
        context_artifacts: BTreeMap::new(),
        responses: BTreeMap::new(),
        synced_transcript_records: BTreeMap::new(),
        synced_response_records: BTreeMap::new(),
        rag_chunks: BTreeMap::new(),
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
    for artifact in context_artifacts {
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
        context.push(context_artifact_from_cloud(data_dir, client, local_id, artifact).await?);
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
    let meeting = MeetingRecord {
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
    sync_state.rag_chunks = restored_rag_states(&meeting, &sync_state);
    write_cloud_sync_state(data_dir, id, &sync_state)?;
    Ok(meeting)
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
    {
        if child_session_id != session_id {
            anyhow::bail!(
                "cloud session bundle child parent mismatch: expected {session_id}, got {child_session_id}"
            );
        }
    }

    let artifact_ids = bundle
        .context_artifacts
        .iter()
        .map(|record| record.artifact_id.as_str())
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
    Ok(())
}

async fn context_artifact_from_cloud(
    data_dir: &Path,
    client: Option<&CloudClient>,
    id: Uuid,
    record: SyncContextArtifactRecord,
) -> Result<ContextArtifact> {
    let kind = context_kind_from_cloud(&record.kind);
    let status = processing_status_from_metadata(&record.metadata, record.text_preview.as_deref());
    let preview_path = write_restored_context_preview(data_dir, id, &record)?;
    let object_path = match client {
        Some(client) => download_restored_context_object(data_dir, client, id, &record)
            .await
            .unwrap_or_else(|| preview_path.clone()),
        None => preview_path.clone(),
    };
    let size_bytes = record
        .metadata
        .get("object_size_bytes")
        .or_else(|| record.metadata.get("size_bytes"))
        .and_then(|value| value.as_u64());
    Ok(ContextArtifact {
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
        created_at: record.created_at_ms.to_string(),
        updated_at: record.updated_at_ms.max(record.created_at_ms).to_string(),
    })
}

fn write_restored_context_preview(
    data_dir: &Path,
    id: Uuid,
    record: &SyncContextArtifactRecord,
) -> Result<std::path::PathBuf> {
    let dir = data_dir.join("cloud-restored-context");
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
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

async fn download_restored_context_object(
    data_dir: &Path,
    client: &CloudClient,
    id: Uuid,
    record: &SyncContextArtifactRecord,
) -> Option<PathBuf> {
    record
        .metadata
        .get("object_key")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())?;
    let bytes = match client.download_artifact_object(&record.artifact_id).await {
        Ok(bytes) => bytes,
        Err(error) => {
            debug!(
                artifact_id = %record.artifact_id,
                error = %error,
                "cloud hydration could not download artifact object"
            );
            return None;
        }
    };
    let dir = data_dir.join("cloud-restored-objects");
    if let Err(error) = cue_core::app_paths::create_private_dir(&dir) {
        debug!(
            dir = %dir.display(),
            error = %error,
            "cloud hydration could not create restored object directory"
        );
        return None;
    }
    let filename = restored_object_filename(id, record);
    let path = dir.join(filename);
    if let Err(error) = fs::write(&path, bytes) {
        debug!(
            path = %path.display(),
            error = %error,
            "cloud hydration could not write restored object"
        );
        return None;
    }
    Some(path)
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
    let metadata = merge_metadata(
        state
            .map(|state| state.session_metadata.clone())
            .unwrap_or_else(empty_metadata),
        json!({
            "session_code": short_session_code(meeting.id),
            "sync_revision": session_content_revision(meeting, responses, state),
            "summary": meeting.summary.as_deref(),
            "conversation_memory": {
                "schema_version": CLOUD_CONVERSATION_MEMORY_SCHEMA_VERSION,
                "state": &meeting.conversation_memory,
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
        session_id: wire_session_id(meeting, state),
        title: meeting.title.clone(),
        status: if meeting.ended_at.is_some() {
            "archived".into()
        } else {
            "active".into()
        },
        created_at_ms: parse_ms(&meeting.started_at),
        updated_at_ms,
        last_active_at_ms: Some(updated_at_ms),
        answer_style: meeting.answer_instructions.clone(),
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

    fn empty_cloud_sync_state(remote_session_id: impl Into<String>) -> CloudSyncState {
        CloudSyncState {
            schema_version: CLOUD_SYNC_STATE_SCHEMA_VERSION,
            remote_session_id: remote_session_id.into(),
            session_metadata: json!({}),
            transcript_segments: BTreeMap::new(),
            context_artifacts: BTreeMap::new(),
            responses: BTreeMap::new(),
            synced_transcript_records: BTreeMap::new(),
            synced_response_records: BTreeMap::new(),
            rag_chunks: BTreeMap::new(),
        }
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
        let bundle_dir = root.join(SESSION_AUDIT_DIR).join(session_id.to_string());
        let event_dir = root
            .join(SESSION_AUDIT_EVENTS_DIR)
            .join(session_id.to_string());
        let marker = root
            .join(SESSION_AUDIT_UPLOADED_DIR)
            .join(format!("{session_id}.json"));
        fs::create_dir_all(&bundle_dir).unwrap();
        fs::create_dir_all(&event_dir).unwrap();
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(bundle_dir.join("bundle.json"), b"{}").unwrap();
        fs::write(event_dir.join("events.jsonl"), b"{}\n").unwrap();
        fs::write(&marker, b"{}").unwrap();

        purge_session_audit_state(&root, session_id).unwrap();
        assert!(!bundle_dir.exists());
        assert!(!event_dir.exists());
        assert!(!marker.exists());
        // Repeated deletion is idempotent.
        purge_session_audit_state(&root, session_id).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn owned_session_delete_is_conservatively_queued_without_sync_state() {
        let root = std::env::temp_dir().join(format!("bluey-local-delete-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let session_id = Uuid::new_v4();
        let disposition = prepare_cloud_session_delete(&root, session_id, "acct-local").unwrap();
        assert_eq!(disposition, CloudSessionDeleteDisposition::Queued);
        let queued: PendingCloudSessionDelete =
            serde_json::from_slice(&fs::read(cloud_delete_outbox_path(&root, session_id)).unwrap())
                .unwrap();
        assert_eq!(queued.remote_session_id, session_id.to_string());
        assert_eq!(queued.state, CloudSessionDeleteState::Prepared);
        abort_prepared_cloud_session_delete(&root, session_id, "acct-local").unwrap();
        assert!(!cloud_delete_outbox_path(&root, session_id).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uploaded_session_delete_uses_durable_remote_provenance() {
        let root = std::env::temp_dir().join(format!("bluey-cloud-delete-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let session_id = Uuid::new_v4();
        write_cloud_sync_state(
            &root,
            session_id,
            &empty_cloud_sync_state("remote-session-1"),
        )
        .unwrap();

        let disposition = prepare_cloud_session_delete(&root, session_id, "acct-1").unwrap();
        assert_eq!(disposition, CloudSessionDeleteDisposition::Queued);
        let queued: PendingCloudSessionDelete =
            serde_json::from_slice(&fs::read(cloud_delete_outbox_path(&root, session_id)).unwrap())
                .unwrap();
        assert_eq!(queued.local_session_id, session_id);
        assert_eq!(queued.remote_session_id, "remote-session-1");
        assert_eq!(queued.owner_account_id, "acct-1");
        assert_eq!(queued.state, CloudSessionDeleteState::Prepared);
        assert!(cloud_sync_state_path(&root, session_id).exists());
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
        validate_cloud_delete_intent(&pending, session_id, "acct-legacy").unwrap();
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
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-rollback")).await;
        assert!(client.deleted_remote_session_ids.lock().unwrap().is_empty());
        let pending = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
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
        assert!(!cloud_delete_outbox_path(&root, session_id).exists());
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
            session_id,
            &empty_cloud_sync_state("remote-recover-1"),
        )
        .unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-recover").unwrap();
        assert!(store.delete(session_id).unwrap());

        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-recover")).unwrap();
        let recovered = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
            .unwrap()
            .expect("reconciled delete intent");
        assert_eq!(recovered.state, CloudSessionDeleteState::Committed);

        let client = RecordingCloudDeleteClient::default();
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-recover")).await;
        flush_pending_cloud_session_deletes_with(&root, &client, Some("acct-recover")).await;
        assert_eq!(
            *client.deleted_remote_session_ids.lock().unwrap(),
            vec!["remote-recover-1".to_string()]
        );
        assert!(!cloud_delete_outbox_path(&root, session_id).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_delete_for_another_account_remains_inert() {
        let root = std::env::temp_dir().join(format!("bluey-delete-owner-{}", Uuid::new_v4()));
        let store = test_meeting_store(&root);
        let session_id = Uuid::new_v4();
        prepare_cloud_session_delete(&root, session_id, "acct-original").unwrap();

        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-other")).unwrap();

        let pending = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
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
        let prepared = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
            .unwrap()
            .expect("signed-out startup leaves the intent inert");
        assert_eq!(prepared.state, CloudSessionDeleteState::Prepared);

        // The post-login reconciliation path can now prove both ownership and
        // the absence of the canonical local record.
        reconcile_prepared_cloud_session_deletes(&root, &store, Some("acct-login")).unwrap();
        let committed = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
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
        let pending = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
            .unwrap()
            .expect("in-flight intent remains prepared");
        assert_eq!(pending.state, CloudSessionDeleteState::Prepared);

        drop(transaction);
        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("reconciliation completes after local transaction");
        worker.join().unwrap();
        assert!(!cloud_delete_outbox_path(&root, session_id).exists());
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
            session_id,
            &empty_cloud_sync_state("remote-delete-1"),
        )
        .unwrap();
        prepare_cloud_session_delete(&root, session_id, "acct-commit").unwrap();
        assert!(store.delete(session_id).unwrap());
        commit_prepared_cloud_session_delete(&root, session_id, "acct-commit").unwrap();
        let committed = read_cloud_delete_intent(&cloud_delete_outbox_path(&root, session_id))
            .unwrap()
            .expect("committed intent");
        assert_eq!(committed.state, CloudSessionDeleteState::Committed);
        assert!(committed.committed_at_ms.is_some());

        let client = RecordingCloudDeleteClient::default();
        assert_eq!(
            flush_queued_cloud_session_delete_with(&root, session_id, &client, "acct-commit",)
                .await,
            CloudSessionDeleteDisposition::Confirmed
        );
        assert_eq!(
            *client.deleted_remote_session_ids.lock().unwrap(),
            vec!["remote-delete-1".to_string()]
        );
        assert!(!cloud_delete_outbox_path(&root, session_id).exists());
        assert!(!cloud_sync_state_path(&root, session_id).exists());
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
                .with_text_preview("the spec mentions blue ocean cache invalidation"),
        );

        let batches = build_sync_batches(&[meeting], &HashMap::new(), &HashMap::new());
        let batch = &batches[0];
        assert_eq!(batch.context_artifacts.len(), 1);
        assert_eq!(batch.rag_chunks.len(), 1);
        assert_eq!(batch.rag_chunks[0].source_kind, "context");
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
        let state = cloud_sync_state_after_upload(&meeting, None, &HashMap::new(), None, None);
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
        let state = cloud_sync_state_after_upload(&meeting, None, &HashMap::new(), None, None);

        write_cloud_sync_state(&root, meeting.id, &state).expect("initial sync state");
        write_cloud_sync_state(&root, meeting.id, &state).expect("state with backup");
        fs::write(cloud_sync_state_path(&root, meeting.id), b"{interrupted")
            .expect("simulate interrupted legacy write");

        let recovered = load_cloud_sync_state(&root, meeting.id).expect("last valid state backup");
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
        };

        let restored = meeting_from_cloud_bundle(&root, None, bundle)
            .await
            .expect("restore meeting");
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

        let built =
            build_local_session_audit_bundle(&root, &meeting, &[response], Some("acct_audit"))
                .expect("audit bundle");

        assert!(built.local_dir.join("manifest.json").is_file());
        assert!(built.local_dir.join("events.jsonl").is_file());
        assert!(built.local_dir.join("questions.jsonl").is_file());
        assert!(built.local_dir.join("responses.jsonl").is_file());
        assert!(built.local_dir.join("transcript.jsonl").is_file());
        assert!(built.local_dir.join("context.jsonl").is_file());
        assert!(built.local_dir.join("screen.jsonl").is_file());
        assert!(built.local_dir.join("artifacts.jsonl").is_file());
        assert!(built.local_dir.join("costs.jsonl").is_file());
        assert!(built.local_dir.join("attachments.jsonl").is_file());
        assert!(built.local_dir.join("audio/audio.jsonl").is_file());
        assert!(!built.bundle.questions.is_empty());
        assert!(!built.bundle.responses.is_empty());
        assert!(!built.bundle.transcript.is_empty());
        assert!(!built.bundle.context.is_empty());
        assert!(!built.bundle.screen.is_empty());
        assert!(!built.bundle.artifacts.is_empty());
        assert!(!built.bundle.costs.is_empty());
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
    fn session_audit_bundle_includes_metadata_only_ui_events() {
        let root = std::env::temp_dir().join(format!("bluey-audit-events-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("audit test root");

        let mut meeting = MeetingRecord::new(Some("Visible glitch".into()));
        meeting.owner_account_id = Some("acct_events".into());
        append_session_audit_event(
            &root,
            &meeting,
            "ui_answer_status",
            json!({ "message": "Reading screen context" }),
        )
        .expect("append status");
        append_session_audit_event(
            &root,
            &meeting,
            "ui_answer_error",
            json!({ "visible_message": "Bluey could not complete that answer yet." }),
        )
        .expect("append error");

        let event_dir = session_audit_event_dir(&root, meeting.id);
        assert!(event_dir.join("events.jsonl").is_file());

        let response = crate::llm::CueResponse::new(
            "answer",
            "Partial answer before the visible error.".into(),
            &meeting.id.to_string(),
            Some("Why did it fail?".into()),
        );
        let built =
            build_local_session_audit_bundle(&root, &meeting, &[response], Some("acct_events"))
                .expect("audit bundle");

        let raw_ui_events = built
            .bundle
            .events
            .iter()
            .filter(|event| {
                event
                    .get("source")
                    .and_then(Value::as_str)
                    .is_some_and(|source| source == "desktop_ui")
            })
            .collect::<Vec<_>>();
        assert_eq!(raw_ui_events.len(), 2);
        assert!(built.bundle.events.iter().any(|event| {
            event.get("kind").and_then(Value::as_str) == Some("ui_answer_error")
                && event
                    .get("payload")
                    .and_then(|payload| payload.get("visible_message_chars"))
                    .and_then(Value::as_u64)
                    .is_some_and(|chars| chars > 0)
        }));
        let serialized = String::from_utf8(built.bytes.clone()).expect("audit bundle utf-8");
        assert!(!serialized.contains("Reading screen context"));
        assert!(!serialized.contains("Bluey could not complete that answer yet."));
        assert!(!serialized.contains("Partial answer before the visible error."));
        assert!(!serialized.contains("Why did it fail?"));
        assert_eq!(
            built
                .bundle
                .manifest
                .get("record_counts")
                .and_then(|counts| counts.get("raw_ui_events"))
                .and_then(Value::as_u64),
            Some(2)
        );

        remove_session_audit_event_log(&root, meeting.id).expect("remove event log");
        assert!(!event_dir.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_audit_event_log_is_bounded_without_cloud_sync() {
        let root = std::env::temp_dir().join(format!("bluey-audit-cap-{}", Uuid::new_v4()));
        let meeting = MeetingRecord::new(Some("Bounded diagnostics".into()));
        let event_dir = session_audit_event_dir(&root, meeting.id);
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

        append_session_audit_event(
            &root,
            &meeting,
            "bounded",
            json!({ "provider": "x".repeat(MAX_AUDIT_EVENT_RECORD_BYTES * 2) }),
        )
        .unwrap();

        let metadata = std::fs::metadata(&event_path).unwrap();
        let events = read_session_audit_events(&root, meeting.id);
        assert!(metadata.len() <= MAX_AUDIT_EVENT_LOG_BYTES);
        assert!(events.len() as u64 <= MAX_AUDIT_EVENT_RECORDS);
        assert_eq!(
            events.last().and_then(|event| event["sequence"].as_u64()),
            Some(MAX_AUDIT_EVENT_RECORDS + 33)
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event["payload"]["reason_code"].as_str()),
            Some("record_size_cap")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn audit_compaction_discards_oversized_legacy_lines_without_unbounded_reads() {
        let root = std::env::temp_dir().join(format!("bluey-audit-line-{}", Uuid::new_v4()));
        let meeting = MeetingRecord::new(Some("Bounded migration".into()));
        let event_dir = session_audit_event_dir(&root, meeting.id);
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

        append_session_audit_event(&root, &meeting, "next", json!({ "success": true })).unwrap();

        let events = read_session_audit_events(&root, meeting.id);
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
            reconcile_cloud_meeting(local, cloud).expect("reconcile conversation");
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
        assert!(reconcile_cloud_meeting(local, cloud).is_err());
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
        meeting.context.push(image);
        meeting.context.push(doc);

        remove_bluey_owned_context_files(&root, &meeting);

        assert!(!image_path.exists());
        assert!(!markdown_path.exists());
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
                    "processing_status": "ready"
                }),
            }],
        };

        let meeting = meeting_from_cloud_bundle(&root, None, bundle)
            .await
            .expect("restore meeting");
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
        assert!(meeting.context[0]
            .markdown_path
            .as_ref()
            .is_some_and(|path| std::path::Path::new(path).exists()));
        assert_eq!(meeting.conversation.len(), 1);
        assert_eq!(meeting.conversation[0].attachment_ids, vec![artifact_id]);

        let _ = std::fs::remove_dir_all(root);
    }
}
