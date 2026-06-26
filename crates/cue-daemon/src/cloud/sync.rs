//! Best-effort local-to-cloud sync for sessions, transcript, answers, and RAG.
//!
//! The desktop remains local-first: every capture is written locally before
//! this module tries the managed cloud. Sync batches are idempotent and small
//! enough for the server's request limits, so retries are safe.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_cloud_client::{
    CloudClient, CloudSessionBundle, SyncBatchRequest, SyncBatchResponse,
    SyncContextArtifactRecord, SyncCounts, SyncCueResponseRecord, SyncRagChunkRecord,
    SyncSessionRecord, SyncTranscriptSegment,
};
use cue_core::{
    ContextArtifact, ContextKind, ContextProcessingStatus, ConversationTurn, MeetingRecord,
    Speaker, TranscriptSegment,
};
use serde_json::json;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::db::Database;
use crate::storage::MeetingStore;

const MAX_SYNC_RECORDS_PER_BATCH: usize = 450;
const MAX_TEXT_PREVIEW_CHARS: usize = 16_000;
const MAX_RESPONSE_CHARS: usize = 128_000;
const MAX_OBJECT_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
struct SyncedObjectMetadata {
    object_key: String,
    size_bytes: u64,
    sha256: String,
    content_type: String,
    expires_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LocalSyncSummary {
    pub accepted: SyncCounts,
    pub batches: usize,
    pub server_time_ms: Option<i64>,
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
}

impl CloudHydrationSummary {
    pub fn total_sessions(&self) -> usize {
        self.restored_sessions + self.skipped_sessions
    }
}

pub async fn sync_local_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
) -> Result<LocalSyncSummary> {
    let meetings = store
        .all_meetings()
        .context("failed to load local sessions")?;
    if meetings.is_empty() {
        return Ok(LocalSyncSummary::empty());
    }

    let response_map = load_local_responses(data_dir, &meetings);
    let uploaded_objects = upload_context_objects(data_dir, &meetings, client).await;
    let batches = build_sync_batches(&meetings, &response_map, &uploaded_objects);
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
    Ok(summary)
}

pub async fn hydrate_missing_cloud_meetings(
    store: &MeetingStore,
    data_dir: &Path,
    client: &CloudClient,
    limit: i64,
) -> Result<CloudHydrationSummary> {
    let response = client
        .list_cloud_sessions(Some(limit.clamp(1, 200)))
        .await
        .context("list cloud sessions")?;
    if response.sessions.is_empty() {
        return Ok(CloudHydrationSummary::default());
    }

    let mut summary = CloudHydrationSummary::default();
    for session in response.sessions {
        let Ok(session_id) = Uuid::parse_str(&session.session_id) else {
            summary.skipped_sessions += 1;
            continue;
        };
        if store.load_by_id(session_id)?.is_some() {
            summary.skipped_sessions += 1;
            continue;
        }

        let bundle = client
            .load_cloud_session(&session.session_id)
            .await
            .with_context(|| format!("load cloud session {}", session.session_id))?;
        let meeting = meeting_from_cloud_bundle(data_dir, Some(client), bundle).await?;
        store.save_archived(&meeting)?;
        summary.restored_sessions += 1;
    }
    Ok(summary)
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
    client: &CloudClient,
) -> HashMap<Uuid, SyncedObjectMetadata> {
    let mut uploaded = HashMap::new();
    let mut seen = HashSet::new();
    for meeting in meetings {
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
            match tokio::fs::read(&path).await {
                Ok(bytes) => match client
                    .upload_artifact_object(&artifact.id.to_string(), bytes, &content_type)
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

fn build_sync_batches(
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
) -> Vec<SyncBatchRequest> {
    let mut batches = Vec::new();

    for meeting in meetings {
        let session = session_record(meeting);
        let mut batch = SyncBatchRequest::default();
        batch.sessions.push(session.clone());

        let mut seen_response_ids = HashSet::new();

        for segment in &meeting.transcript {
            maybe_flush(&mut batches, &mut batch, &session);
            batch
                .transcript_segments
                .push(transcript_record(meeting, segment));

            if segment.is_final {
                if let Some(chunk) = transcript_rag_chunk(meeting, segment) {
                    maybe_flush(&mut batches, &mut batch, &session);
                    batch.rag_chunks.push(chunk);
                }
            }
        }

        for artifact in &meeting.context {
            maybe_flush(&mut batches, &mut batch, &session);
            batch
                .context_artifacts
                .push(context_record(meeting, artifact, uploaded_objects));
            if let Some(chunk) = context_rag_chunk(meeting, artifact) {
                maybe_flush(&mut batches, &mut batch, &session);
                batch.rag_chunks.push(chunk);
            }
        }

        if let Some(summary) = meeting.summary.as_deref().and_then(truncate_nonempty) {
            maybe_flush(&mut batches, &mut batch, &session);
            batch.rag_chunks.push(SyncRagChunkRecord {
                chunk_id: format!("{}:summary:0", meeting.id),
                session_id: Some(meeting.id.to_string()),
                source_kind: "summary".into(),
                source_id: meeting.id.to_string(),
                chunk_index: 0,
                text: summary,
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: updated_at_ms(meeting),
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
                chunk_id: format!("{}:instructions:0", meeting.id),
                session_id: Some(meeting.id.to_string()),
                source_kind: "answer_instructions".into(),
                source_id: meeting.id.to_string(),
                chunk_index: 0,
                text: instructions,
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: updated_at_ms(meeting),
                metadata: json!({}),
            });
        }

        if let Some(responses) = response_map.get(&meeting.id.to_string()) {
            for response in responses {
                seen_response_ids.insert(response.id.clone());
                maybe_flush(&mut batches, &mut batch, &session);
                batch.cue_responses.push(cue_response_record(response));
                if let Some(chunk) = response_rag_chunk(meeting, response) {
                    maybe_flush(&mut batches, &mut batch, &session);
                    batch.rag_chunks.push(chunk);
                }
            }
        }

        for turn in &meeting.conversation {
            let fallback_id = format!("turn-{}", turn.id);
            if seen_response_ids.contains(&fallback_id) {
                continue;
            }
            maybe_flush(&mut batches, &mut batch, &session);
            batch
                .cue_responses
                .push(conversation_response_record(meeting, turn));
            if let Some(chunk) = conversation_rag_chunk(meeting, turn) {
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

async fn meeting_from_cloud_bundle(
    data_dir: &Path,
    client: Option<&CloudClient>,
    bundle: CloudSessionBundle,
) -> Result<MeetingRecord> {
    let id = Uuid::parse_str(&bundle.session.session_id)
        .with_context(|| format!("invalid cloud session id {}", bundle.session.session_id))?;
    let mut transcript = Vec::new();
    for segment in bundle.transcript_segments {
        transcript.push(TranscriptSegment {
            id: Uuid::parse_str(&segment.segment_id).unwrap_or_else(|_| Uuid::new_v4()),
            speaker: speaker_from_cloud(&segment.speaker, &segment.source),
            text: segment.text,
            created_at: segment.ts_ms.to_string(),
            is_final: segment.is_final,
        });
    }

    let mut context = Vec::new();
    for artifact in bundle.context_artifacts {
        context.push(context_artifact_from_cloud(data_dir, client, artifact).await?);
    }

    let mut conversation = Vec::new();
    for response in bundle.cue_responses {
        if let Some(turn) = conversation_turn_from_cloud(response) {
            conversation.push(turn);
        }
    }

    Ok(MeetingRecord {
        id,
        title: bundle.session.title,
        started_at: bundle.session.created_at_ms.to_string(),
        ended_at: if bundle.session.status == "active" {
            None
        } else {
            Some(bundle.session.updated_at_ms.to_string())
        },
        transcript,
        action_items: Vec::new(),
        decisions: Vec::new(),
        context,
        conversation,
        answer_instructions: bundle.session.answer_style,
        summary: bundle
            .session
            .metadata
            .get("summary")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
    })
}

async fn context_artifact_from_cloud(
    data_dir: &Path,
    client: Option<&CloudClient>,
    record: SyncContextArtifactRecord,
) -> Result<ContextArtifact> {
    let id = Uuid::parse_str(&record.artifact_id).unwrap_or_else(|_| Uuid::new_v4());
    let kind = context_kind_from_cloud(&record.kind);
    let status = processing_status_from_metadata(&record.metadata, record.text_preview.as_deref());
    let preview_path = write_restored_context_preview(data_dir, id, &record)?;
    let object_path = match client {
        Some(client) => download_restored_context_object(data_dir, client, id, &record)
            .await
            .unwrap_or_else(|| preview_path.clone()),
        None => preview_path.clone(),
    };
    let restored_original = object_path != preview_path;
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
        note: restored_note(record.note, record.source_uri, restored_original),
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

fn restored_note(
    note: Option<String>,
    source_uri: Option<String>,
    restored_original: bool,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(note) = note.filter(|value| !value.trim().is_empty()) {
        parts.push(note);
    }
    if let Some(source_uri) = source_uri.filter(|value| !value.trim().is_empty()) {
        parts.push(format!("Original path on synced device: {source_uri}"));
    }
    if restored_original {
        parts.push("Restored from Bluey Cloud with the original synced file bytes.".to_string());
    } else {
        parts.push("Restored from Bluey Cloud using answer-ready text preview because original file bytes were unavailable.".to_string());
    }
    Some(parts.join("\n"))
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

fn conversation_turn_from_cloud(response: SyncCueResponseRecord) -> Option<ConversationTurn> {
    if response.kind != "answer" {
        return None;
    }
    let question = response.source_text?;
    if question.trim().is_empty() || response.text.trim().is_empty() {
        return None;
    }
    let id = response
        .response_id
        .strip_prefix("turn-")
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    Some(ConversationTurn {
        id,
        question,
        answer: response.text,
        attachment_ids: attachment_ids_from_metadata(&response.metadata),
        source: response
            .metadata
            .get("source")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        provider: response.provider,
        created_at: response.ts_ms.to_string(),
    })
}

fn attachment_ids_from_metadata(metadata: &serde_json::Value) -> Vec<Uuid> {
    metadata
        .get("attachment_ids")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .filter_map(|value| Uuid::parse_str(value).ok())
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

fn session_record(meeting: &MeetingRecord) -> SyncSessionRecord {
    SyncSessionRecord {
        session_id: meeting.id.to_string(),
        title: meeting.title.clone(),
        status: if meeting.ended_at.is_some() {
            "archived".into()
        } else {
            "active".into()
        },
        created_at_ms: parse_ms(&meeting.started_at),
        updated_at_ms: updated_at_ms(meeting),
        last_active_at_ms: Some(updated_at_ms(meeting)),
        answer_style: meeting.answer_instructions.clone(),
        metadata: json!({
            "summary": meeting.summary.as_deref(),
            "action_items": meeting.action_items.len(),
            "decisions": meeting.decisions.len(),
        }),
        deleted_at_ms: None,
    }
}

fn transcript_record(
    meeting: &MeetingRecord,
    segment: &TranscriptSegment,
) -> SyncTranscriptSegment {
    SyncTranscriptSegment {
        segment_id: segment.id.to_string(),
        session_id: meeting.id.to_string(),
        speaker: segment.speaker.to_string(),
        source: match segment.speaker {
            cue_core::Speaker::System => "system",
            cue_core::Speaker::User => "microphone",
            cue_core::Speaker::Other | cue_core::Speaker::Unknown => "unknown",
        }
        .to_string(),
        text: truncate_chars(&segment.text, MAX_TEXT_PREVIEW_CHARS),
        start_ms: None,
        end_ms: None,
        ts_ms: parse_ms(&segment.created_at),
        is_final: segment.is_final,
        metadata: json!({}),
    }
}

fn context_record(
    meeting: &MeetingRecord,
    artifact: &ContextArtifact,
    uploaded_objects: &HashMap<Uuid, SyncedObjectMetadata>,
) -> SyncContextArtifactRecord {
    let uploaded = uploaded_objects.get(&artifact.id);
    SyncContextArtifactRecord {
        artifact_id: artifact.id.to_string(),
        session_id: meeting.id.to_string(),
        kind: artifact.kind.to_string(),
        title: artifact.title.clone(),
        note: artifact.note.clone(),
        source_uri: Some(artifact.path.clone()),
        content_hash: uploaded.map(|object| object.sha256.clone()),
        text_preview: artifact
            .text_preview
            .as_deref()
            .map(|text| truncate_chars(text, MAX_TEXT_PREVIEW_CHARS)),
        created_at_ms: parse_ms(&artifact.created_at),
        metadata: json!({
            "size_bytes": artifact.size_bytes,
            "processing_status": artifact.processing_status.to_string(),
            "processing_error": artifact.processing_error.as_deref(),
            "object_key": uploaded.map(|object| object.object_key.as_str()),
            "object_size_bytes": uploaded.map(|object| object.size_bytes),
            "object_sha256": uploaded.map(|object| object.sha256.as_str()),
            "object_content_type": uploaded.map(|object| object.content_type.as_str()),
            "object_expires_at_ms": uploaded.map(|object| object.expires_at_ms),
        }),
    }
}

fn cue_response_record(response: &crate::llm::CueResponse) -> SyncCueResponseRecord {
    SyncCueResponseRecord {
        response_id: response.id.clone(),
        session_id: response.source_session_id.clone(),
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
        artifact_body: response.artifact_body.clone(),
        artifact_confidence: response.artifact_confidence,
        metadata: json!({
            "input_tokens": response.input_tokens,
            "output_tokens": response.output_tokens,
        }),
    }
}

fn conversation_response_record(
    meeting: &MeetingRecord,
    turn: &ConversationTurn,
) -> SyncCueResponseRecord {
    SyncCueResponseRecord {
        response_id: format!("turn-{}", turn.id),
        session_id: meeting.id.to_string(),
        kind: "answer".into(),
        text: truncate_chars(&turn.answer, MAX_RESPONSE_CHARS),
        source_text: Some(truncate_chars(&turn.question, MAX_TEXT_PREVIEW_CHARS)),
        ts_ms: parse_ms(&turn.created_at),
        provider: turn.provider.clone(),
        model: None,
        lane: None,
        task_type: None,
        cost_cents: None,
        balance_cents_after: None,
        cost_label: None,
        artifact_type: None,
        artifact_body: None,
        artifact_confidence: None,
        metadata: json!({
            "source": turn.source.as_deref(),
            "attachment_ids": turn
                .attachment_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        }),
    }
}

fn transcript_rag_chunk(
    meeting: &MeetingRecord,
    segment: &TranscriptSegment,
) -> Option<SyncRagChunkRecord> {
    Some(SyncRagChunkRecord {
        chunk_id: format!("{}:transcript:{}:0", meeting.id, segment.id),
        session_id: Some(meeting.id.to_string()),
        source_kind: "transcript".into(),
        source_id: segment.id.to_string(),
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
) -> Option<SyncRagChunkRecord> {
    Some(SyncRagChunkRecord {
        chunk_id: format!("{}:context:{}:0", meeting.id, artifact.id),
        session_id: Some(meeting.id.to_string()),
        source_kind: "context".into(),
        source_id: artifact.id.to_string(),
        chunk_index: 0,
        text: truncate_nonempty(artifact.text_preview.as_deref()?)?,
        embedding: None,
        embedding_model: None,
        token_count: None,
        content_hash: None,
        updated_at_ms: parse_ms(&artifact.created_at),
        metadata: json!({ "title": artifact.title, "kind": artifact.kind.to_string() }),
    })
}

fn response_rag_chunk(
    meeting: &MeetingRecord,
    response: &crate::llm::CueResponse,
) -> Option<SyncRagChunkRecord> {
    Some(SyncRagChunkRecord {
        chunk_id: format!("{}:response:{}:0", meeting.id, response.id),
        session_id: Some(meeting.id.to_string()),
        source_kind: "response".into(),
        source_id: response.id.clone(),
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
) -> Option<SyncRagChunkRecord> {
    Some(SyncRagChunkRecord {
        chunk_id: format!("{}:conversation:{}:0", meeting.id, turn.id),
        session_id: Some(meeting.id.to_string()),
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

fn updated_at_ms(meeting: &MeetingRecord) -> i64 {
    let mut latest = parse_ms(&meeting.started_at);
    if let Some(ended_at) = meeting.ended_at.as_deref() {
        latest = latest.max(parse_ms(ended_at));
    }
    for segment in &meeting.transcript {
        latest = latest.max(parse_ms(&segment.created_at));
    }
    for artifact in &meeting.context {
        latest = latest.max(parse_ms(&artifact.created_at));
    }
    for turn in &meeting.conversation {
        latest = latest.max(parse_ms(&turn.created_at));
    }
    latest
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
    fn conversation_sync_metadata_preserves_attachment_ids() {
        let mut meeting = MeetingRecord::new(Some("Chat".into()));
        let attachment_id = Uuid::new_v4();
        let turn = ConversationTurn::new("question", "answer", None, Some("test".into()))
            .with_attachment_ids(vec![attachment_id]);

        let record = conversation_response_record(&meeting, &turn);
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
