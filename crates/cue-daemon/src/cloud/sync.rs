//! Best-effort local-to-cloud sync for sessions, transcript, answers, and RAG.
//!
//! The desktop remains local-first: every capture is written locally before
//! this module tries the managed cloud. Sync batches are idempotent and small
//! enough for the server's request limits, so retries are safe.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use cue_cloud_client::{
    CloudClient, SyncBatchRequest, SyncBatchResponse, SyncContextArtifactRecord, SyncCounts,
    SyncCueResponseRecord, SyncRagChunkRecord, SyncSessionRecord, SyncTranscriptSegment,
};
use cue_core::{ContextArtifact, ConversationTurn, MeetingRecord, TranscriptSegment};
use serde_json::json;
use tracing::debug;

use crate::db::Database;
use crate::storage::MeetingStore;

const MAX_SYNC_RECORDS_PER_BATCH: usize = 450;
const MAX_TEXT_PREVIEW_CHARS: usize = 16_000;
const MAX_RESPONSE_CHARS: usize = 128_000;
const INTERNAL_WARMUP_SOURCE: &str = "warmup";

fn is_syncable_conversation_turn(turn: &ConversationTurn) -> bool {
    turn.source.as_deref() != Some(INTERNAL_WARMUP_SOURCE)
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
    let batches = build_sync_batches(&meetings, &response_map);
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

fn build_sync_batches(
    meetings: &[MeetingRecord],
    response_map: &HashMap<String, Vec<crate::llm::CueResponse>>,
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
                .push(context_record(meeting, artifact));
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

        for turn in meeting
            .conversation
            .iter()
            .filter(|turn| is_syncable_conversation_turn(turn))
        {
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
) -> SyncContextArtifactRecord {
    SyncContextArtifactRecord {
        artifact_id: artifact.id.to_string(),
        session_id: meeting.id.to_string(),
        kind: artifact.kind.to_string(),
        title: artifact.title.clone(),
        note: artifact.note.clone(),
        source_uri: Some(artifact.path.clone()),
        content_hash: None,
        text_preview: artifact
            .text_preview
            .as_deref()
            .map(|text| truncate_chars(text, MAX_TEXT_PREVIEW_CHARS)),
        created_at_ms: parse_ms(&artifact.created_at),
        metadata: json!({
            "size_bytes": artifact.size_bytes,
            "processing_status": artifact.processing_status.to_string(),
            "processing_error": artifact.processing_error.as_deref(),
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
        metadata: json!({ "source": turn.source.as_deref() }),
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

        let batches = build_sync_batches(&[meeting], &HashMap::new());
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

        let batches = build_sync_batches(&[meeting], &HashMap::new());
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

        let batches = build_sync_batches(&[meeting], &HashMap::new());
        assert_eq!(batches[0].cue_responses.len(), 1);
        assert!(batches[0].cue_responses[0].response_id.starts_with("turn-"));
    }

    #[test]
    fn internal_warmup_turn_is_never_synced_or_indexed() {
        let secret_prompt = "Provider event ID: google-private\nAttendee: person@example.com";
        let mut meeting = MeetingRecord::new(Some("Private prep".into()));
        meeting.conversation.push(ConversationTurn::new(
            secret_prompt,
            "Ready.",
            Some(INTERNAL_WARMUP_SOURCE.into()),
            Some("test".into()),
        ));

        let batches = build_sync_batches(&[meeting], &HashMap::new());
        assert_eq!(batches.len(), 1);
        assert!(batches[0].cue_responses.is_empty());
        assert!(batches[0].rag_chunks.is_empty());
        let encoded = serde_json::to_string(&batches).expect("serialize sync batches");
        assert!(!encoded.contains("google-private"));
        assert!(!encoded.contains("person@example.com"));
    }
}
