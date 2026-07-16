//! Best-effort local-to-cloud sync for sessions, transcript, answers, and RAG.
//!
//! The desktop remains local-first: every capture is written locally before
//! this module tries the managed cloud. Sync batches are idempotent and small
//! enough for the server's request limits, so retries are safe.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_cloud_client::{
    CloudClient, CloudSessionBundle, SessionAuditBundleResponse, SyncBatchRequest,
    SyncBatchResponse, SyncContextArtifactRecord, SyncCounts, SyncCueResponseRecord,
    SyncRagChunkRecord, SyncSessionRecord, SyncTranscriptSegment,
};
use cue_core::{
    short_session_code, CardArtifactType, ContextArtifact, ContextKind, ContextProcessingStatus,
    ConversationTurn, CueCardArtifact, MeetingDiagnostics, MeetingRecord, Speaker,
    TranscriptSegment,
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
const DEFAULT_AUDIT_LOCAL_RETENTION_DAYS: i64 = 7;
const DEFAULT_AUDIT_LOCAL_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SESSION_AUDIT_DIR: &str = "session-audit";
const SESSION_AUDIT_EVENTS_DIR: &str = "session-audit-events";
const SESSION_AUDIT_UPLOADED_DIR: &str = "session-audit-uploaded";
const CLOUD_SYNC_STATE_DIR: &str = "cloud-sync-state";
const CLOUD_SYNC_STATE_SCHEMA_VERSION: u32 = 1;

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
    let session_id = meeting.id.to_string();
    let session_code = short_session_code(meeting.id);
    let device_id = std::env::var("BLUEY_DEVICE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let event_dir = session_audit_event_dir(data_dir, meeting.id);
    cue_core::app_paths::create_private_dir(&event_dir)?;
    let event_path = event_dir.join("events.jsonl");
    let sequence = next_audit_event_sequence(&event_path).saturating_add(1);
    let event_id = format!(
        "{session_code}-ui-{sequence:08}-{}",
        Uuid::new_v4().simple()
    );
    let record = json!({
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
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&event_path)
        .with_context(|| format!("open {}", event_path.display()))?;
    serde_json::to_writer(&mut file, &record)
        .with_context(|| format!("write {}", event_path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("write {}", event_path.display()))?;
    file.flush()
        .with_context(|| format!("flush {}", event_path.display()))?;
    Ok(())
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
    let uploaded_objects = upload_context_objects(data_dir, &meetings, &sync_states, client).await;
    let batches =
        build_sync_batches_with_states(&meetings, &response_map, &uploaded_objects, &sync_states);
    if batches.is_empty() {
        return Ok(LocalSyncSummary::empty());
    }

    let mut summary = LocalSyncSummary::empty();
    for batch in batches {
        let response = client
            .sync_batch(&batch)
            .await
            .context("cloud sync batch")?;
        summary.add_response(response);
    }
    sync_session_audit_bundles(data_dir, &meetings, &response_map, client, owner_account_id).await;
    Ok(summary)
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
    let before = serde_json::to_value(&local)?;
    let local_was_shell = !meeting_has_syncable_content(&local, None);

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
    if local.last_error_message.is_none() {
        local.last_error_message = cloud.last_error_message;
    }
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
                    .upload_artifact_object(&artifact_id, bytes, &content_type)
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

fn next_audit_event_sequence(event_path: &Path) -> u64 {
    let Ok(file) = fs::File::open(event_path) else {
        return 0;
    };
    BufReader::new(file)
        .lines()
        .map_while(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .count()
        .min(u64::MAX as usize) as u64
}

fn read_session_audit_events(data_dir: &Path, session_id: Uuid) -> Vec<Value> {
    let event_path = session_audit_event_dir(data_dir, session_id).join("events.jsonl");
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

fn load_cloud_sync_state(data_dir: &Path, session_id: Uuid) -> Option<CloudSyncState> {
    let path = cloud_sync_state_path(data_dir, session_id);
    let bytes = fs::read(&path).ok()?;
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
    write_json_file(&cloud_sync_state_path(data_dir, session_id), state)
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
        if !meeting_has_syncable_content(meeting, responses) {
            debug!(
                session_id = %meeting.id,
                title = %meeting.title,
                "cloud sync skipped empty local meeting shell"
            );
            continue;
        }

        let sync_state = sync_states.get(&meeting.id);
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
                metadata: json!({}),
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
        sync_state.transcript_segments.insert(
            local_id.to_string(),
            CloudTranscriptState {
                record_id: segment.segment_id.clone(),
                speaker: segment.speaker.clone(),
                source: segment.source.clone(),
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                ts_ms: segment.ts_ms,
                metadata: segment.metadata.clone(),
            },
        );
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
                metadata: artifact.metadata.clone(),
            },
        );
        context.push(context_artifact_from_cloud(data_dir, client, local_id, artifact).await?);
    }

    let mut conversation = Vec::new();
    for response in cue_responses {
        let local_id = local_turn_uuid(&remote_session_id, &response.response_id);
        if let Some(turn) = conversation_turn_from_cloud(response.clone(), local_id) {
            sync_state.responses.insert(
                local_id.to_string(),
                CloudResponseState {
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
                },
            );
            conversation.push(turn);
        }
    }

    let live_answer_transcript_cursor = transcript.len();
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
        answer_instructions: session.answer_style,
        diagnostics: session
            .metadata
            .get("diagnostics")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        summary: session
            .metadata
            .get("summary")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
    };
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
                "last_error_message": meeting.diagnostics.last_error_message.as_deref(),
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
        artifact_id: wire_context_artifact_id(meeting, artifact.id, state),
        session_id: wire_session_id(meeting, state),
        kind: artifact.kind.to_string(),
        title: artifact.title.clone(),
        note: artifact.note.clone(),
        source_uri: preserved
            .and_then(|record| record.source_uri.clone())
            .or_else(|| Some(artifact.path.clone())),
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
        metadata,
    }
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
    use cue_core::{ContextKind, Speaker};

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
