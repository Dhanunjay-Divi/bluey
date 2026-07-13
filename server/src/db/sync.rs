//! Cloud sync + RAG persistence helpers.
//!
//! The SQLite path backs local/dev alpha installs, while the Postgres path is
//! the server-side sync and cloud RAG runtime target.

use anyhow::{Context, Result};
use postgres::{Client, Row as PgRow};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::DbPool;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncSessionRecord {
    pub session_id: String,
    pub title: String,
    #[serde(default = "default_status")]
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub last_active_at_ms: Option<i64>,
    #[serde(default)]
    pub answer_style: Option<String>,
    #[serde(default = "empty_json")]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncTranscriptSegment {
    pub segment_id: String,
    pub session_id: String,
    pub speaker: String,
    pub source: String,
    pub text: String,
    #[serde(default)]
    pub start_ms: Option<i64>,
    #[serde(default)]
    pub end_ms: Option<i64>,
    pub ts_ms: i64,
    #[serde(default = "default_true")]
    pub is_final: bool,
    #[serde(default = "empty_json")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncCueResponseRecord {
    pub response_id: String,
    pub session_id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub source_text: Option<String>,
    pub ts_ms: i64,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub lane: Option<String>,
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub cost_cents: Option<i64>,
    #[serde(default)]
    pub balance_cents_after: Option<i64>,
    #[serde(default)]
    pub cost_label: Option<String>,
    #[serde(default)]
    pub artifact_type: Option<String>,
    #[serde(default)]
    pub artifact_body: Option<String>,
    #[serde(default)]
    pub artifact_confidence: Option<f32>,
    #[serde(default = "empty_json")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncContextArtifactRecord {
    pub artifact_id: String,
    pub session_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub text_preview: Option<String>,
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default = "empty_json")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncRagChunkRecord {
    pub chunk_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub source_kind: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub text: String,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub token_count: Option<i64>,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub updated_at_ms: i64,
    #[serde(default = "empty_json")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncCounts {
    pub sessions: usize,
    pub transcript_segments: usize,
    pub cue_responses: usize,
    pub context_artifacts: usize,
    pub rag_chunks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudSessionSummary {
    pub session_id: String,
    pub title: String,
    pub status: String,
    pub updated_at_ms: i64,
    pub last_active_at_ms: Option<i64>,
    pub answer_style: Option<String>,
    pub transcript_count: i64,
    pub response_count: i64,
    pub context_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudDeletedSession {
    pub session_id: String,
    pub deleted_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudSessionBundle {
    pub session: SyncSessionRecord,
    pub transcript_segments: Vec<SyncTranscriptSegment>,
    pub cue_responses: Vec<SyncCueResponseRecord>,
    pub context_artifacts: Vec<SyncContextArtifactRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RagMatch {
    pub chunk_id: String,
    pub session_id: Option<String>,
    pub source_kind: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub text: String,
    pub score: f32,
    pub embedding_model: Option<String>,
}

pub fn upsert_batch(
    pool: &DbPool,
    account_id: &str,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<SyncCounts> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => upsert_batch_sqlite(
            pool,
            account_id,
            sessions,
            transcript_segments,
            cue_responses,
            context_artifacts,
            rag_chunks,
        ),
        DbPool::Postgres(_) => upsert_batch_postgres(
            pool,
            account_id,
            sessions,
            transcript_segments,
            cue_responses,
            context_artifacts,
            rag_chunks,
        ),
    })
}

fn upsert_batch_sqlite(
    pool: &DbPool,
    account_id: &str,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<SyncCounts> {
    let mut conn = pool.get().context("get db conn")?;
    let tx = conn.transaction().context("begin sync tx")?;
    let mut applied_transcript_segments = 0;
    let mut applied_cue_responses = 0;
    let mut applied_context_artifacts = 0;
    let mut applied_rag_chunks = 0;

    for record in sessions {
        let metadata = serde_json::to_string(&record.metadata)?;
        tx.execute(
            "INSERT INTO cloud_sessions (
                account_id, session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json, deleted_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(account_id, session_id) DO UPDATE SET
                title=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.title
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.title
                    ELSE excluded.title
                END,
                status=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.status
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.status
                    ELSE excluded.status
                END,
                updated_at_ms=MAX(cloud_sessions.updated_at_ms, excluded.updated_at_ms),
                last_active_at_ms=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.last_active_at_ms
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.last_active_at_ms
                    ELSE excluded.last_active_at_ms
                END,
                answer_style=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.answer_style
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.answer_style
                    ELSE excluded.answer_style
                END,
                metadata_json=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.metadata_json
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.metadata_json
                    ELSE excluded.metadata_json
                END,
                deleted_at_ms=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.deleted_at_ms
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.deleted_at_ms
                    ELSE excluded.deleted_at_ms
                END",
            params![
                account_id,
                record.session_id,
                record.title,
                record.status,
                record.created_at_ms,
                record.updated_at_ms,
                record.last_active_at_ms,
                record.answer_style,
                metadata,
                record.deleted_at_ms,
            ],
        )?;
    }

    for record in transcript_segments {
        if session_is_tombstoned_sqlite_tx(&tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        applied_transcript_segments += tx.execute(
            "INSERT INTO cloud_transcript_segments (
                account_id, segment_id, session_id, speaker, source, text,
                start_ms, end_ms, ts_ms, is_final, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(account_id, segment_id) DO UPDATE SET
                session_id=excluded.session_id,
                speaker=excluded.speaker,
                source=excluded.source,
                text=excluded.text,
                start_ms=excluded.start_ms,
                end_ms=excluded.end_ms,
                ts_ms=excluded.ts_ms,
                is_final=excluded.is_final,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_transcript_segments.ts_ms",
            params![
                account_id,
                record.segment_id,
                record.session_id,
                record.speaker,
                record.source,
                record.text,
                record.start_ms,
                record.end_ms,
                record.ts_ms,
                if record.is_final { 1 } else { 0 },
                metadata,
            ],
        )?;
    }

    for record in cue_responses {
        if session_is_tombstoned_sqlite_tx(&tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        applied_cue_responses += tx.execute(
            "INSERT INTO cloud_cue_responses (
                account_id, response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(account_id, response_id) DO UPDATE SET
                session_id=excluded.session_id,
                kind=excluded.kind,
                text=excluded.text,
                source_text=excluded.source_text,
                ts_ms=excluded.ts_ms,
                provider=excluded.provider,
                model=excluded.model,
                lane=excluded.lane,
                task_type=excluded.task_type,
                cost_cents=excluded.cost_cents,
                balance_cents_after=excluded.balance_cents_after,
                cost_label=excluded.cost_label,
                artifact_type=excluded.artifact_type,
                artifact_body=excluded.artifact_body,
                artifact_confidence=excluded.artifact_confidence,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_cue_responses.ts_ms",
            params![
                account_id,
                record.response_id,
                record.session_id,
                record.kind,
                record.text,
                record.source_text,
                record.ts_ms,
                record.provider,
                record.model,
                record.lane,
                record.task_type,
                record.cost_cents,
                record.balance_cents_after,
                record.cost_label,
                record.artifact_type,
                record.artifact_body,
                record.artifact_confidence,
                metadata,
            ],
        )?;
    }

    for record in context_artifacts {
        if session_is_tombstoned_sqlite_tx(&tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let updated_at_ms = record.updated_at_ms.max(record.created_at_ms);
        let affected = tx.execute(
            "INSERT INTO cloud_context_artifacts (
                account_id, artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(account_id, artifact_id) DO UPDATE SET
                session_id=excluded.session_id,
                kind=excluded.kind,
                title=excluded.title,
                note=excluded.note,
                source_uri=excluded.source_uri,
                content_hash=excluded.content_hash,
                text_preview=excluded.text_preview,
                created_at_ms=excluded.created_at_ms,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms > cloud_context_artifacts.updated_at_ms",
            params![
                account_id,
                record.artifact_id,
                record.session_id,
                record.kind,
                record.title,
                record.note,
                record.source_uri,
                record.content_hash,
                record.text_preview,
                record.created_at_ms,
                updated_at_ms,
                metadata,
            ],
        )?;
        if affected > 0 {
            applied_context_artifacts += affected;
            crate::db::object_uploads::link_artifact_session_sqlite_tx(
                &tx,
                account_id,
                &record.artifact_id,
                &record.session_id,
                now_ms(),
            )?;
        }
    }

    for record in rag_chunks {
        if let Some(session_id) = record.session_id.as_deref() {
            if session_is_tombstoned_sqlite_tx(&tx, account_id, session_id)? {
                continue;
            }
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let embedding = record
            .embedding
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        applied_rag_chunks += tx.execute(
            "INSERT INTO cloud_rag_chunks (
                account_id, chunk_id, session_id, source_kind, source_id, chunk_index,
                text, embedding_json, embedding_model, token_count, content_hash,
                updated_at_ms, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(account_id, chunk_id) DO UPDATE SET
                session_id=excluded.session_id,
                source_kind=excluded.source_kind,
                source_id=excluded.source_id,
                chunk_index=excluded.chunk_index,
                text=excluded.text,
                embedding_json=excluded.embedding_json,
                embedding_model=excluded.embedding_model,
                token_count=excluded.token_count,
                content_hash=excluded.content_hash,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms >= cloud_rag_chunks.updated_at_ms",
            params![
                account_id,
                record.chunk_id,
                record.session_id,
                record.source_kind,
                record.source_id,
                record.chunk_index,
                record.text,
                embedding,
                record.embedding_model,
                record.token_count,
                record.content_hash,
                record.updated_at_ms,
                metadata,
            ],
        )?;
    }

    tx.commit().context("commit sync tx")?;
    Ok(SyncCounts {
        sessions: sessions.len(),
        transcript_segments: applied_transcript_segments,
        cue_responses: applied_cue_responses,
        context_artifacts: applied_context_artifacts,
        rag_chunks: applied_rag_chunks,
    })
}

fn upsert_batch_postgres(
    pool: &DbPool,
    account_id: &str,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<SyncCounts> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let mut tx = conn.transaction().context("begin sync postgres tx")?;
    let mut applied_transcript_segments = 0;
    let mut applied_cue_responses = 0;
    let mut applied_context_artifacts = 0;
    let mut applied_rag_chunks = 0;

    for record in sessions {
        let metadata = serde_json::to_string(&record.metadata)?;
        let title = db_text(&record.title);
        let status = db_text(&record.status);
        let answer_style = db_opt_text(&record.answer_style);
        tx.execute(
            "INSERT INTO cloud_sessions (
                account_id, session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json, deleted_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT(account_id, session_id) DO UPDATE SET
                title=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.title
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.title
                    ELSE excluded.title
                END,
                status=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.status
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.status
                    ELSE excluded.status
                END,
                updated_at_ms=GREATEST(cloud_sessions.updated_at_ms, excluded.updated_at_ms),
                last_active_at_ms=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.last_active_at_ms
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.last_active_at_ms
                    ELSE excluded.last_active_at_ms
                END,
                answer_style=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.answer_style
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.answer_style
                    ELSE excluded.answer_style
                END,
                metadata_json=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.metadata_json
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.metadata_json
                    ELSE excluded.metadata_json
                END,
                deleted_at_ms=CASE
                    WHEN cloud_sessions.deleted_at_ms IS NOT NULL AND excluded.deleted_at_ms IS NULL
                        THEN cloud_sessions.deleted_at_ms
                    WHEN excluded.updated_at_ms < cloud_sessions.updated_at_ms
                        THEN cloud_sessions.deleted_at_ms
                    ELSE excluded.deleted_at_ms
                END",
            &[
                &account_id,
                &record.session_id,
                &title,
                &status,
                &record.created_at_ms,
                &record.updated_at_ms,
                &record.last_active_at_ms,
                &answer_style,
                &metadata,
                &record.deleted_at_ms,
            ],
        )
        .with_context(|| format!("upsert cloud_sessions session_id={}", record.session_id))?;
    }

    for record in transcript_segments {
        if session_is_tombstoned_postgres_tx(&mut tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let is_final = if record.is_final { 1_i32 } else { 0_i32 };
        let speaker = db_text(&record.speaker);
        let source = db_text(&record.source);
        let text = db_text(&record.text);
        applied_transcript_segments += tx.execute(
            "INSERT INTO cloud_transcript_segments (
                account_id, segment_id, session_id, speaker, source, text,
                start_ms, end_ms, ts_ms, is_final, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(account_id, segment_id) DO UPDATE SET
                session_id=excluded.session_id,
                speaker=excluded.speaker,
                source=excluded.source,
                text=excluded.text,
                start_ms=excluded.start_ms,
                end_ms=excluded.end_ms,
                ts_ms=excluded.ts_ms,
                is_final=excluded.is_final,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_transcript_segments.ts_ms",
            &[
                &account_id,
                &record.segment_id,
                &record.session_id,
                &speaker,
                &source,
                &text,
                &record.start_ms,
                &record.end_ms,
                &record.ts_ms,
                &is_final,
                &metadata,
            ],
        )
        .with_context(|| {
            format!(
                "upsert cloud_transcript_segments segment_id={} session_id={}",
                record.segment_id, record.session_id
            )
        })? as usize;
    }

    for record in cue_responses {
        if session_is_tombstoned_postgres_tx(&mut tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let artifact_confidence = record.artifact_confidence.map(|v| v as f64);
        let kind = db_text(&record.kind);
        let text = db_text(&record.text);
        let source_text = db_opt_text(&record.source_text);
        let provider = db_opt_text(&record.provider);
        let model = db_opt_text(&record.model);
        let lane = db_opt_text(&record.lane);
        let task_type = db_opt_text(&record.task_type);
        let cost_label = db_opt_text(&record.cost_label);
        let artifact_type = db_opt_text(&record.artifact_type);
        let artifact_body = db_opt_text(&record.artifact_body);
        applied_cue_responses += tx.execute(
            "INSERT INTO cloud_cue_responses (
                account_id, response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
             ON CONFLICT(account_id, response_id) DO UPDATE SET
                session_id=excluded.session_id,
                kind=excluded.kind,
                text=excluded.text,
                source_text=excluded.source_text,
                ts_ms=excluded.ts_ms,
                provider=excluded.provider,
                model=excluded.model,
                lane=excluded.lane,
                task_type=excluded.task_type,
                cost_cents=excluded.cost_cents,
                balance_cents_after=excluded.balance_cents_after,
                cost_label=excluded.cost_label,
                artifact_type=excluded.artifact_type,
                artifact_body=excluded.artifact_body,
                artifact_confidence=excluded.artifact_confidence,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_cue_responses.ts_ms",
            &[
                &account_id,
                &record.response_id,
                &record.session_id,
                &kind,
                &text,
                &source_text,
                &record.ts_ms,
                &provider,
                &model,
                &lane,
                &task_type,
                &record.cost_cents,
                &record.balance_cents_after,
                &cost_label,
                &artifact_type,
                &artifact_body,
                &artifact_confidence,
                &metadata,
            ],
        )
        .with_context(|| {
            format!(
                "upsert cloud_cue_responses response_id={} session_id={}",
                record.response_id, record.session_id
            )
        })? as usize;
    }

    for record in context_artifacts {
        if session_is_tombstoned_postgres_tx(&mut tx, account_id, &record.session_id)? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let kind = db_text(&record.kind);
        let title = db_text(&record.title);
        let note = db_opt_text(&record.note);
        let source_uri = db_opt_text(&record.source_uri);
        let content_hash = db_opt_text(&record.content_hash);
        let text_preview = db_opt_text(&record.text_preview);
        let updated_at_ms = record.updated_at_ms.max(record.created_at_ms);
        let affected = tx.execute(
            "INSERT INTO cloud_context_artifacts (
                account_id, artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT(account_id, artifact_id) DO UPDATE SET
                session_id=excluded.session_id,
                kind=excluded.kind,
                title=excluded.title,
                note=excluded.note,
                source_uri=excluded.source_uri,
                content_hash=excluded.content_hash,
                text_preview=excluded.text_preview,
                created_at_ms=excluded.created_at_ms,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms > cloud_context_artifacts.updated_at_ms",
            &[
                &account_id,
                &record.artifact_id,
                &record.session_id,
                &kind,
                &title,
                &note,
                &source_uri,
                &content_hash,
                &text_preview,
                &record.created_at_ms,
                &updated_at_ms,
                &metadata,
            ],
        )
        .with_context(|| {
            format!(
                "upsert cloud_context_artifacts artifact_id={} session_id={}",
                record.artifact_id, record.session_id
            )
        })?;
        if affected > 0 {
            applied_context_artifacts += affected as usize;
            crate::db::object_uploads::link_artifact_session_postgres_tx(
                &mut tx,
                account_id,
                &record.artifact_id,
                &record.session_id,
                now_ms(),
            )?;
        }
    }

    for record in rag_chunks {
        if let Some(session_id) = record.session_id.as_deref() {
            if session_is_tombstoned_postgres_tx(&mut tx, account_id, session_id)? {
                continue;
            }
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let embedding_json = record
            .embedding
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let embedding_vector = record.embedding.as_deref().and_then(vector_literal_1536);
        let source_kind = db_text(&record.source_kind);
        let source_id = db_text(&record.source_id);
        let text = db_text(&record.text);
        let embedding_model = db_opt_text(&record.embedding_model);
        let content_hash = db_opt_text(&record.content_hash);
        let chunk_index = i32::try_from(record.chunk_index).with_context(|| {
            format!(
                "cloud_rag_chunks chunk_index out of range chunk_id={} value={}",
                record.chunk_id, record.chunk_index
            )
        })?;
        let token_count = record
            .token_count
            .map(|value| {
                i32::try_from(value).with_context(|| {
                    format!(
                        "cloud_rag_chunks token_count out of range chunk_id={} value={}",
                        record.chunk_id, value
                    )
                })
            })
            .transpose()?;
        applied_rag_chunks += tx.execute(
            "INSERT INTO cloud_rag_chunks (
                account_id, chunk_id, session_id, source_kind, source_id, chunk_index,
                text, embedding_json, embedding, embedding_model, token_count, content_hash,
                updated_at_ms, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::vector, $10, $11, $12, $13, $14)
             ON CONFLICT(account_id, chunk_id) DO UPDATE SET
                session_id=excluded.session_id,
                source_kind=excluded.source_kind,
                source_id=excluded.source_id,
                chunk_index=excluded.chunk_index,
                text=excluded.text,
                embedding_json=excluded.embedding_json,
                embedding=excluded.embedding,
                embedding_model=excluded.embedding_model,
                token_count=excluded.token_count,
                content_hash=excluded.content_hash,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms >= cloud_rag_chunks.updated_at_ms",
            &[
                &account_id,
                &record.chunk_id,
                &record.session_id,
                &source_kind,
                &source_id,
                &chunk_index,
                &text,
                &embedding_json,
                &embedding_vector,
                &embedding_model,
                &token_count,
                &content_hash,
                &record.updated_at_ms,
                &metadata,
            ],
        )
        .with_context(|| {
            format!(
                "upsert cloud_rag_chunks chunk_id={} source_id={}",
                record.chunk_id, record.source_id
            )
        })? as usize;
    }

    tx.commit().context("commit sync postgres tx")?;
    Ok(SyncCounts {
        sessions: sessions.len(),
        transcript_segments: applied_transcript_segments,
        cue_responses: applied_cue_responses,
        context_artifacts: applied_context_artifacts,
        rag_chunks: applied_rag_chunks,
    })
}

pub fn list_sessions(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudSessionSummary>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => list_sessions_sqlite(pool, account_id, limit),
        DbPool::Postgres(_) => list_sessions_postgres(pool, account_id, limit),
    })
}

pub fn list_deleted_sessions(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudDeletedSession>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => list_deleted_sessions_sqlite(pool, account_id, limit),
        DbPool::Postgres(_) => list_deleted_sessions_postgres(pool, account_id, limit),
    })
}

pub fn tombstone_session(pool: &DbPool, account_id: &str, session_id: &str) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => tombstone_session_sqlite(pool, account_id, session_id),
        DbPool::Postgres(_) => tombstone_session_postgres(pool, account_id, session_id),
    })
}

fn list_deleted_sessions_sqlite(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudDeletedSession>> {
    let conn = pool.get().context("get db conn")?;
    let mut stmt = conn.prepare(
        "SELECT session_id, COALESCE(deleted_at_ms, updated_at_ms), updated_at_ms
         FROM cloud_sessions
         WHERE account_id = ?1 AND deleted_at_ms IS NOT NULL
         ORDER BY COALESCE(deleted_at_ms, updated_at_ms) DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit], |row| {
        Ok(CloudDeletedSession {
            session_id: row.get(0)?,
            deleted_at_ms: row.get(1)?,
            updated_at_ms: row.get(2)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn list_deleted_sessions_postgres(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudDeletedSession>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let rows = conn.query(
        "SELECT session_id, COALESCE(deleted_at_ms, updated_at_ms), updated_at_ms
         FROM cloud_sessions
         WHERE account_id = $1 AND deleted_at_ms IS NOT NULL
         ORDER BY COALESCE(deleted_at_ms, updated_at_ms) DESC
         LIMIT $2",
        &[&account_id, &limit],
    )?;
    rows.into_iter()
        .map(|row| {
            Ok(CloudDeletedSession {
                session_id: row.try_get(0)?,
                deleted_at_ms: row.try_get(1)?,
                updated_at_ms: row.try_get(2)?,
            })
        })
        .collect()
}

fn tombstone_session_sqlite(pool: &DbPool, account_id: &str, session_id: &str) -> Result<()> {
    let mut conn = pool.get().context("get db conn")?;
    let tx = conn.transaction().context("begin session tombstone tx")?;
    let now = now_ms();
    tx.execute(
        "INSERT INTO cloud_sessions (
            account_id, session_id, title, status, created_at_ms, updated_at_ms,
            last_active_at_ms, answer_style, metadata_json, deleted_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)
         ON CONFLICT(account_id, session_id) DO UPDATE SET
            status='deleted',
            updated_at_ms=MAX(cloud_sessions.updated_at_ms, excluded.updated_at_ms),
            deleted_at_ms=COALESCE(cloud_sessions.deleted_at_ms, excluded.deleted_at_ms)",
        params![
            account_id,
            session_id,
            "Deleted session",
            "deleted",
            now,
            now,
            now,
            "{}",
            now,
        ],
    )?;
    crate::db::object_uploads::schedule_session_cleanup_sqlite_tx(
        &tx, account_id, session_id, now,
    )?;
    purge_session_content_sqlite_tx(&tx, account_id, session_id)?;
    tx.commit().context("commit session tombstone tx")?;
    Ok(())
}

fn tombstone_session_postgres(pool: &DbPool, account_id: &str, session_id: &str) -> Result<()> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres session tombstone tx")?;
    let now = now_ms();
    let title = db_text("Deleted session");
    let status = db_text("deleted");
    let metadata = "{}".to_string();
    tx.execute(
        "INSERT INTO cloud_sessions (
            account_id, session_id, title, status, created_at_ms, updated_at_ms,
            last_active_at_ms, answer_style, metadata_json, deleted_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9)
         ON CONFLICT(account_id, session_id) DO UPDATE SET
            status='deleted',
            updated_at_ms=GREATEST(cloud_sessions.updated_at_ms, excluded.updated_at_ms),
            deleted_at_ms=COALESCE(cloud_sessions.deleted_at_ms, excluded.deleted_at_ms)",
        &[
            &account_id,
            &session_id,
            &title,
            &status,
            &now,
            &now,
            &now,
            &metadata,
            &now,
        ],
    )
    .with_context(|| format!("tombstone cloud_sessions session_id={session_id}"))?;
    crate::db::object_uploads::schedule_session_cleanup_postgres_tx(
        &mut tx, account_id, session_id, now,
    )?;
    purge_session_content_postgres_tx(&mut tx, account_id, session_id)?;
    tx.commit()
        .context("commit postgres session tombstone tx")?;
    Ok(())
}

fn purge_session_content_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<()> {
    for table in [
        "cloud_transcript_segments",
        "cloud_cue_responses",
        "cloud_context_artifacts",
        "cloud_rag_chunks",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE account_id = ?1 AND session_id = ?2"),
            params![account_id, session_id],
        )?;
    }
    Ok(())
}

fn session_is_tombstoned_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM cloud_sessions
             WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NOT NULL",
            params![account_id, session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn purge_session_content_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<()> {
    for table in [
        "cloud_transcript_segments",
        "cloud_cue_responses",
        "cloud_context_artifacts",
        "cloud_rag_chunks",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE account_id = $1 AND session_id = $2"),
            &[&account_id, &session_id],
        )?;
    }
    Ok(())
}

fn session_is_tombstoned_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_opt(
            "SELECT 1 FROM cloud_sessions
             WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NOT NULL",
            &[&account_id, &session_id],
        )?
        .is_some())
}

fn list_sessions_sqlite(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudSessionSummary>> {
    let conn = pool.get().context("get db conn")?;
    let mut stmt = conn.prepare(
        "SELECT s.session_id, s.title, s.status, s.updated_at_ms, s.last_active_at_ms,
                s.answer_style,
                (SELECT COUNT(*) FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id),
                (SELECT COUNT(*) FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id),
                (SELECT COUNT(*) FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
         FROM cloud_sessions s
         WHERE s.account_id = ?1 AND s.deleted_at_ms IS NULL
           AND (
                EXISTS (SELECT 1 FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
           )
         ORDER BY s.updated_at_ms DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit], |row| {
        Ok(CloudSessionSummary {
            session_id: row.get(0)?,
            title: row.get(1)?,
            status: row.get(2)?,
            updated_at_ms: row.get(3)?,
            last_active_at_ms: row.get(4)?,
            answer_style: row.get(5)?,
            transcript_count: row.get(6)?,
            response_count: row.get(7)?,
            context_count: row.get(8)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn list_sessions_postgres(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudSessionSummary>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let rows = conn.query(
        "SELECT s.session_id, s.title, s.status, s.updated_at_ms, s.last_active_at_ms,
                s.answer_style,
                (SELECT COUNT(*)::bigint FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id),
                (SELECT COUNT(*)::bigint FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id),
                (SELECT COUNT(*)::bigint FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
         FROM cloud_sessions s
         WHERE s.account_id = $1 AND s.deleted_at_ms IS NULL
           AND (
                EXISTS (SELECT 1 FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
           )
         ORDER BY s.updated_at_ms DESC
         LIMIT $2",
        &[&account_id, &limit],
    )?;
    rows.into_iter()
        .map(cloud_session_summary_from_pg)
        .collect()
}

pub fn load_session(
    pool: &DbPool,
    account_id: &str,
    session_id: &str,
) -> Result<Option<CloudSessionBundle>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => load_session_sqlite(pool, account_id, session_id),
        DbPool::Postgres(_) => load_session_postgres(pool, account_id, session_id),
    })
}

pub fn load_context_artifact(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<SyncContextArtifactRecord>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => load_context_artifact_sqlite(pool, account_id, artifact_id),
        DbPool::Postgres(_) => load_context_artifact_postgres(pool, account_id, artifact_id),
    })
}

fn load_session_sqlite(
    pool: &DbPool,
    account_id: &str,
    session_id: &str,
) -> Result<Option<CloudSessionBundle>> {
    let conn = pool.get().context("get db conn")?;
    let session = conn
        .query_row(
            "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, answer_style, metadata_json, deleted_at_ms
             FROM cloud_sessions
             WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NULL",
            params![account_id, session_id],
            |row| {
                let metadata: String = row.get(7)?;
                Ok(SyncSessionRecord {
                    session_id: row.get(0)?,
                    title: row.get(1)?,
                    status: row.get(2)?,
                    created_at_ms: row.get(3)?,
                    updated_at_ms: row.get(4)?,
                    last_active_at_ms: row.get(5)?,
                    answer_style: row.get(6)?,
                    metadata: parse_json(&metadata),
                    deleted_at_ms: row.get(8)?,
                })
            },
        )
        .optional()?;

    let Some(session) = session else {
        return Ok(None);
    };

    Ok(Some(CloudSessionBundle {
        transcript_segments: load_transcripts(&conn, account_id, session_id)?,
        cue_responses: load_responses(&conn, account_id, session_id)?,
        context_artifacts: load_context(&conn, account_id, session_id)?,
        session,
    }))
}

fn load_session_postgres(
    pool: &DbPool,
    account_id: &str,
    session_id: &str,
) -> Result<Option<CloudSessionBundle>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let session = conn
        .query_opt(
            "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, answer_style, metadata_json, deleted_at_ms
             FROM cloud_sessions
             WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NULL",
            &[&account_id, &session_id],
        )?
        .map(sync_session_from_pg)
        .transpose()?;

    let Some(session) = session else {
        return Ok(None);
    };

    Ok(Some(CloudSessionBundle {
        transcript_segments: load_transcripts_pg(&mut conn, account_id, session_id)?,
        cue_responses: load_responses_pg(&mut conn, account_id, session_id)?,
        context_artifacts: load_context_pg(&mut conn, account_id, session_id)?,
        session,
    }))
}

fn load_context_artifact_sqlite(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<SyncContextArtifactRecord>> {
    let conn = pool.get().context("get db conn")?;
    conn.query_row(
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = ?1 AND artifact_id = ?2",
        params![account_id, artifact_id],
        |row| {
            let metadata: String = row.get(10)?;
            Ok(SyncContextArtifactRecord {
                artifact_id: row.get(0)?,
                session_id: row.get(1)?,
                kind: row.get(2)?,
                title: row.get(3)?,
                note: row.get(4)?,
                source_uri: row.get(5)?,
                content_hash: row.get(6)?,
                text_preview: row.get(7)?,
                created_at_ms: row.get(8)?,
                updated_at_ms: row.get(9)?,
                metadata: parse_json(&metadata),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn load_context_artifact_postgres(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<SyncContextArtifactRecord>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let row = conn.query_opt(
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = $1 AND artifact_id = $2",
        &[&account_id, &artifact_id],
    )?;
    row.map(|row| {
        let metadata: String = row.try_get(10)?;
        Ok(SyncContextArtifactRecord {
            artifact_id: row.try_get(0)?,
            session_id: row.try_get(1)?,
            kind: row.try_get(2)?,
            title: row.try_get(3)?,
            note: row.try_get(4)?,
            source_uri: row.try_get(5)?,
            content_hash: row.try_get(6)?,
            text_preview: row.try_get(7)?,
            created_at_ms: row.try_get(8)?,
            updated_at_ms: row.try_get(9)?,
            metadata: parse_json(&metadata),
        })
    })
    .transpose()
}

pub fn query_rag(
    pool: &DbPool,
    account_id: &str,
    query: &str,
    embedding: Option<&[f32]>,
    top_k: i64,
) -> Result<Vec<RagMatch>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => query_rag_sqlite(pool, account_id, query, embedding, top_k),
        DbPool::Postgres(_) => query_rag_postgres(pool, account_id, query, embedding, top_k),
    })
}

fn query_rag_sqlite(
    pool: &DbPool,
    account_id: &str,
    query: &str,
    embedding: Option<&[f32]>,
    top_k: i64,
) -> Result<Vec<RagMatch>> {
    let conn = pool.get().context("get db conn")?;
    let mut stmt = conn.prepare(
        "SELECT chunk_id, session_id, source_kind, source_id, chunk_index,
                text, embedding_json, embedding_model
         FROM cloud_rag_chunks c
         WHERE c.account_id = ?1
           AND NOT EXISTS (
             SELECT 1 FROM cloud_sessions s
              WHERE s.account_id = c.account_id
                AND s.session_id = c.session_id
                AND s.deleted_at_ms IS NOT NULL
           )
         ORDER BY c.updated_at_ms DESC
         LIMIT 2000",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;

    let query_terms = terms(query);
    let mut scored = Vec::new();
    for row in rows {
        let (
            chunk_id,
            session_id,
            source_kind,
            source_id,
            chunk_index,
            text,
            embedding_json,
            embedding_model,
        ) = row?;
        let vector_score = match (embedding, embedding_json.as_deref()) {
            (Some(q), Some(json)) => serde_json::from_str::<Vec<f32>>(json)
                .ok()
                .and_then(|v| cosine(q, &v))
                .unwrap_or(0.0),
            _ => 0.0,
        };
        let lexical_score = lexical_overlap(&query_terms, &terms(&text));
        let score = if embedding.is_some() {
            (0.85 * vector_score) + (0.15 * lexical_score)
        } else {
            lexical_score
        };
        if score > 0.0 {
            scored.push(RagMatch {
                chunk_id,
                session_id,
                source_kind,
                source_id,
                chunk_index,
                text,
                score,
                embedding_model,
            });
        }
    }
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(top_k.clamp(1, 20) as usize);
    Ok(scored)
}

fn query_rag_postgres(
    pool: &DbPool,
    account_id: &str,
    query: &str,
    embedding: Option<&[f32]>,
    top_k: i64,
) -> Result<Vec<RagMatch>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let rows = conn.query(
        "SELECT c.chunk_id, c.session_id, c.source_kind, c.source_id, c.chunk_index,
                c.text, c.embedding_json, c.embedding_model
         FROM cloud_rag_chunks c
         WHERE c.account_id = $1
           AND NOT EXISTS (
             SELECT 1 FROM cloud_sessions s
              WHERE s.account_id = c.account_id
                AND s.session_id = c.session_id
                AND s.deleted_at_ms IS NOT NULL
           )
         ORDER BY c.updated_at_ms DESC
         LIMIT 2000",
        &[&account_id],
    )?;

    let query_terms = terms(query);
    let mut scored = Vec::new();
    for row in rows {
        let chunk_id: String = row.try_get(0)?;
        let session_id: Option<String> = row.try_get(1)?;
        let source_kind: String = row.try_get(2)?;
        let source_id: String = row.try_get(3)?;
        let chunk_index: i32 = row.try_get(4)?;
        let text: String = row.try_get(5)?;
        let embedding_json: Option<String> = row.try_get(6)?;
        let embedding_model: Option<String> = row.try_get(7)?;
        let vector_score = match (embedding, embedding_json.as_deref()) {
            (Some(q), Some(json)) => serde_json::from_str::<Vec<f32>>(json)
                .ok()
                .and_then(|v| cosine(q, &v))
                .unwrap_or(0.0),
            _ => 0.0,
        };
        let lexical_score = lexical_overlap(&query_terms, &terms(&text));
        let score = if embedding.is_some() {
            (0.85 * vector_score) + (0.15 * lexical_score)
        } else {
            lexical_score
        };
        if score > 0.0 {
            scored.push(RagMatch {
                chunk_id,
                session_id,
                source_kind,
                source_id,
                chunk_index: i64::from(chunk_index),
                text,
                score,
                embedding_model,
            });
        }
    }
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(top_k.clamp(1, 20) as usize);
    Ok(scored)
}

fn load_transcripts(
    conn: &rusqlite::Connection,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncTranscriptSegment>> {
    let mut stmt = conn.prepare(
        "SELECT segment_id, session_id, speaker, source, text, start_ms, end_ms,
                ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments
         WHERE account_id = ?1 AND session_id = ?2
         ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![account_id, session_id], |row| {
        let metadata: String = row.get(9)?;
        Ok(SyncTranscriptSegment {
            segment_id: row.get(0)?,
            session_id: row.get(1)?,
            speaker: row.get(2)?,
            source: row.get(3)?,
            text: row.get(4)?,
            start_ms: row.get(5)?,
            end_ms: row.get(6)?,
            ts_ms: row.get(7)?,
            is_final: row.get::<_, i64>(8)? != 0,
            metadata: parse_json(&metadata),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn load_responses(
    conn: &rusqlite::Connection,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncCueResponseRecord>> {
    let mut stmt = conn.prepare(
        "SELECT response_id, session_id, kind, text, source_text, ts_ms, provider,
                model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence, metadata_json
         FROM cloud_cue_responses
         WHERE account_id = ?1 AND session_id = ?2
         ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![account_id, session_id], |row| {
        let metadata: String = row.get(16)?;
        Ok(SyncCueResponseRecord {
            response_id: row.get(0)?,
            session_id: row.get(1)?,
            kind: row.get(2)?,
            text: row.get(3)?,
            source_text: row.get(4)?,
            ts_ms: row.get(5)?,
            provider: row.get(6)?,
            model: row.get(7)?,
            lane: row.get(8)?,
            task_type: row.get(9)?,
            cost_cents: row.get(10)?,
            balance_cents_after: row.get(11)?,
            cost_label: row.get(12)?,
            artifact_type: row.get(13)?,
            artifact_body: row.get(14)?,
            artifact_confidence: row.get(15)?,
            metadata: parse_json(&metadata),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn load_context(
    conn: &rusqlite::Connection,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncContextArtifactRecord>> {
    let mut stmt = conn.prepare(
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = ?1 AND session_id = ?2
         ORDER BY created_at_ms ASC",
    )?;
    let rows = stmt.query_map(params![account_id, session_id], |row| {
        let metadata: String = row.get(10)?;
        Ok(SyncContextArtifactRecord {
            artifact_id: row.get(0)?,
            session_id: row.get(1)?,
            kind: row.get(2)?,
            title: row.get(3)?,
            note: row.get(4)?,
            source_uri: row.get(5)?,
            content_hash: row.get(6)?,
            text_preview: row.get(7)?,
            created_at_ms: row.get(8)?,
            updated_at_ms: row.get(9)?,
            metadata: parse_json(&metadata),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn cloud_session_summary_from_pg(row: PgRow) -> Result<CloudSessionSummary> {
    Ok(CloudSessionSummary {
        session_id: row.try_get(0)?,
        title: row.try_get(1)?,
        status: row.try_get(2)?,
        updated_at_ms: row.try_get(3)?,
        last_active_at_ms: row.try_get(4)?,
        answer_style: row.try_get(5)?,
        transcript_count: row.try_get(6)?,
        response_count: row.try_get(7)?,
        context_count: row.try_get(8)?,
    })
}

fn sync_session_from_pg(row: PgRow) -> Result<SyncSessionRecord> {
    let metadata: String = row.try_get(7)?;
    Ok(SyncSessionRecord {
        session_id: row.try_get(0)?,
        title: row.try_get(1)?,
        status: row.try_get(2)?,
        created_at_ms: row.try_get(3)?,
        updated_at_ms: row.try_get(4)?,
        last_active_at_ms: row.try_get(5)?,
        answer_style: row.try_get(6)?,
        metadata: parse_json(&metadata),
        deleted_at_ms: row.try_get(8)?,
    })
}

fn load_transcripts_pg(
    conn: &mut Client,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncTranscriptSegment>> {
    let rows = conn.query(
        "SELECT segment_id, session_id, speaker, source, text, start_ms, end_ms,
                ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments
         WHERE account_id = $1 AND session_id = $2
         ORDER BY ts_ms ASC",
        &[&account_id, &session_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let metadata: String = row.try_get(9)?;
            let is_final: i32 = row.try_get(8)?;
            Ok(SyncTranscriptSegment {
                segment_id: row.try_get(0)?,
                session_id: row.try_get(1)?,
                speaker: row.try_get(2)?,
                source: row.try_get(3)?,
                text: row.try_get(4)?,
                start_ms: row.try_get(5)?,
                end_ms: row.try_get(6)?,
                ts_ms: row.try_get(7)?,
                is_final: is_final != 0,
                metadata: parse_json(&metadata),
            })
        })
        .collect()
}

fn load_responses_pg(
    conn: &mut Client,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncCueResponseRecord>> {
    let rows = conn.query(
        "SELECT response_id, session_id, kind, text, source_text, ts_ms, provider,
                model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence, metadata_json
         FROM cloud_cue_responses
         WHERE account_id = $1 AND session_id = $2
         ORDER BY ts_ms ASC",
        &[&account_id, &session_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let metadata: String = row.try_get(16)?;
            let artifact_confidence: Option<f64> = row.try_get(15)?;
            Ok(SyncCueResponseRecord {
                response_id: row.try_get(0)?,
                session_id: row.try_get(1)?,
                kind: row.try_get(2)?,
                text: row.try_get(3)?,
                source_text: row.try_get(4)?,
                ts_ms: row.try_get(5)?,
                provider: row.try_get(6)?,
                model: row.try_get(7)?,
                lane: row.try_get(8)?,
                task_type: row.try_get(9)?,
                cost_cents: row.try_get(10)?,
                balance_cents_after: row.try_get(11)?,
                cost_label: row.try_get(12)?,
                artifact_type: row.try_get(13)?,
                artifact_body: row.try_get(14)?,
                artifact_confidence: artifact_confidence.map(|v| v as f32),
                metadata: parse_json(&metadata),
            })
        })
        .collect()
}

fn load_context_pg(
    conn: &mut Client,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncContextArtifactRecord>> {
    let rows = conn.query(
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = $1 AND session_id = $2
         ORDER BY created_at_ms ASC",
        &[&account_id, &session_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let metadata: String = row.try_get(10)?;
            Ok(SyncContextArtifactRecord {
                artifact_id: row.try_get(0)?,
                session_id: row.try_get(1)?,
                kind: row.try_get(2)?,
                title: row.try_get(3)?,
                note: row.try_get(4)?,
                source_uri: row.try_get(5)?,
                content_hash: row.try_get(6)?,
                text_preview: row.try_get(7)?,
                created_at_ms: row.try_get(8)?,
                updated_at_ms: row.try_get(9)?,
                metadata: parse_json(&metadata),
            })
        })
        .collect()
}

fn vector_literal_1536(values: &[f32]) -> Option<String> {
    if values.len() != 1536 {
        return None;
    }
    let body = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("[{body}]"))
}

fn db_text(value: &str) -> String {
    value.replace('\0', "")
}

fn db_opt_text(value: &Option<String>) -> Option<String> {
    value.as_deref().map(db_text)
}

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| empty_json())
}

fn empty_json() -> serde_json::Value {
    serde_json::json!({})
}

fn default_status() -> String {
    "active".to_string()
}

fn default_true() -> bool {
    true
}

fn terms(text: &str) -> std::collections::BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|term| {
            let term = term.trim().to_ascii_lowercase();
            (term.len() >= 3).then_some(term)
        })
        .collect()
}

fn lexical_overlap(
    query: &std::collections::BTreeSet<String>,
    text: &std::collections::BTreeSet<String>,
) -> f32 {
    if query.is_empty() || text.is_empty() {
        return 0.0;
    }
    let hits = query.intersection(text).count() as f32;
    hits / query.len().max(1) as f32
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut aa = 0.0f32;
    let mut bb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= f32::EPSILON || bb <= f32::EPSILON {
        return None;
    }
    Some(dot / (aa.sqrt() * bb.sqrt()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    #[test]
    fn db_text_removes_nul_bytes_before_postgres_bind() {
        assert_eq!(db_text("Resume\0.pdf"), "Resume.pdf");
        assert_eq!(
            db_opt_text(&Some("screen\0context".to_string())),
            Some("screencontext".to_string())
        );
        assert_eq!(db_opt_text(&None), None);
    }

    #[test]
    fn sync_batch_round_trips_session_bundle_and_rag() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_1";
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, "a@example.com", "hash"],
            )
            .unwrap();

        let counts = upsert_batch(
            &pool,
            account_id,
            &[SyncSessionRecord {
                session_id: "s1".into(),
                title: "System design".into(),
                status: "active".into(),
                created_at_ms: 1,
                updated_at_ms: 10,
                last_active_at_ms: Some(10),
                answer_style: Some("concise".into()),
                metadata: serde_json::json!({"mode":"system_design"}),
                deleted_at_ms: None,
            }],
            &[SyncTranscriptSegment {
                segment_id: "t1".into(),
                session_id: "s1".into(),
                speaker: "system".into(),
                source: "system".into(),
                text: "design cache invalidation".into(),
                start_ms: None,
                end_ms: None,
                ts_ms: 2,
                is_final: true,
                metadata: serde_json::json!({}),
            }],
            &[SyncCueResponseRecord {
                response_id: "r1".into(),
                session_id: "s1".into(),
                kind: "answer".into(),
                text: "Use write-through cache.".into(),
                source_text: Some("How cache?".into()),
                ts_ms: 3,
                provider: Some("openai".into()),
                model: Some("gpt-4o-mini".into()),
                lane: Some("instant".into()),
                task_type: Some("system_design".into()),
                cost_cents: Some(2),
                balance_cents_after: Some(2998),
                cost_label: Some("$0.02".into()),
                artifact_type: Some("system_design".into()),
                artifact_body: Some("CACHE\n-----".into()),
                artifact_confidence: Some(0.9),
                metadata: serde_json::json!({}),
            }],
            &[],
            &[SyncRagChunkRecord {
                chunk_id: "c1".into(),
                session_id: Some("s1".into()),
                source_kind: "transcript".into(),
                source_id: "t1".into(),
                chunk_index: 0,
                text: "cache invalidation and write-through design".into(),
                embedding: Some(vec![1.0, 0.0]),
                embedding_model: Some("test-embed".into()),
                token_count: Some(6),
                content_hash: None,
                updated_at_ms: 4,
                metadata: serde_json::json!({}),
            }],
        )
        .unwrap();
        assert_eq!(counts.sessions, 1);
        assert_eq!(counts.rag_chunks, 1);

        let sessions = list_sessions(&pool, account_id, 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].transcript_count, 1);
        assert_eq!(sessions[0].response_count, 1);

        let bundle = load_session(&pool, account_id, "s1").unwrap().unwrap();
        assert_eq!(bundle.session.answer_style.as_deref(), Some("concise"));
        assert_eq!(
            bundle.cue_responses[0].artifact_type.as_deref(),
            Some("system_design")
        );

        let matches = query_rag(&pool, account_id, "cache", Some(&[1.0, 0.0]), 5).unwrap();
        assert_eq!(matches[0].chunk_id, "c1");
        assert!(matches[0].score > 0.8);
    }

    #[test]
    fn list_sessions_hides_empty_shells_until_content_arrives() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_shell";
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, "shell@example.com", "hash"],
            )
            .unwrap();

        let session = SyncSessionRecord {
            session_id: "empty-session".into(),
            title: "New recording".into(),
            status: "active".into(),
            created_at_ms: 1,
            updated_at_ms: 10,
            last_active_at_ms: Some(10),
            answer_style: None,
            metadata: serde_json::json!({}),
            deleted_at_ms: None,
        };
        upsert_batch(
            &pool,
            account_id,
            std::slice::from_ref(&session),
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap();

        let sessions = list_sessions(&pool, account_id, 10).unwrap();
        assert!(
            sessions.is_empty(),
            "session-only sync shells should not render as conversations"
        );

        upsert_batch(
            &pool,
            account_id,
            &[session],
            &[],
            &[SyncCueResponseRecord {
                response_id: "turn-1".into(),
                session_id: "empty-session".into(),
                kind: "answer".into(),
                text: "Real answer".into(),
                source_text: Some("Real question".into()),
                ts_ms: 11,
                provider: Some("bluey_managed".into()),
                model: Some("balanced".into()),
                lane: Some("balanced".into()),
                task_type: Some("general".into()),
                cost_cents: None,
                balance_cents_after: None,
                cost_label: None,
                artifact_type: None,
                artifact_body: None,
                artifact_confidence: None,
                metadata: serde_json::json!({}),
            }],
            &[],
            &[],
        )
        .unwrap();

        let sessions = list_sessions(&pool, account_id, 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "empty-session");
        assert_eq!(sessions[0].response_count, 1);
    }

    #[test]
    fn stale_sync_records_do_not_overwrite_newer_session_or_content() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_stale_sync";
        let session_id = "session-stale-sync";
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, "stale@example.com", "hash"],
            )
            .unwrap();

        let session = |title: &str, updated_at_ms: i64| SyncSessionRecord {
            session_id: session_id.into(),
            title: title.into(),
            status: "active".into(),
            created_at_ms: 1,
            updated_at_ms,
            last_active_at_ms: Some(updated_at_ms),
            answer_style: Some(title.into()),
            metadata: serde_json::json!({"version": title}),
            deleted_at_ms: None,
        };
        let transcript = |text: &str, ts_ms: i64| SyncTranscriptSegment {
            segment_id: "segment-versioned".into(),
            session_id: session_id.into(),
            speaker: "user".into(),
            source: "microphone".into(),
            text: text.into(),
            start_ms: None,
            end_ms: None,
            ts_ms,
            is_final: true,
            metadata: serde_json::json!({"version": text}),
        };
        let response = |text: &str, ts_ms: i64| SyncCueResponseRecord {
            response_id: "response-versioned".into(),
            session_id: session_id.into(),
            kind: "answer".into(),
            text: text.into(),
            source_text: Some("question".into()),
            ts_ms,
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
            metadata: serde_json::json!({"version": text}),
        };
        let context = |title: &str, updated_at_ms: i64| SyncContextArtifactRecord {
            artifact_id: "artifact-versioned".into(),
            session_id: session_id.into(),
            kind: "document".into(),
            title: title.into(),
            note: None,
            source_uri: None,
            content_hash: None,
            text_preview: Some(title.into()),
            created_at_ms: 1,
            updated_at_ms,
            metadata: serde_json::json!({"version": title}),
        };
        let rag = |text: &str, updated_at_ms: i64| SyncRagChunkRecord {
            chunk_id: "rag-versioned".into(),
            session_id: Some(session_id.into()),
            source_kind: "document".into(),
            source_id: "artifact-versioned".into(),
            chunk_index: 0,
            text: text.into(),
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms,
            metadata: serde_json::json!({"version": text}),
        };

        let initial_counts = upsert_batch(
            &pool,
            account_id,
            &[session("new", 20)],
            &[transcript("new", 20)],
            &[response("new", 20)],
            &[context("new", 20)],
            &[rag("new", 20)],
        )
        .unwrap();
        assert_eq!(initial_counts.transcript_segments, 1);
        assert_eq!(initial_counts.cue_responses, 1);
        assert_eq!(initial_counts.context_artifacts, 1);
        assert_eq!(initial_counts.rag_chunks, 1);

        let stale_counts = upsert_batch(
            &pool,
            account_id,
            &[session("old", 10)],
            &[transcript("old", 10)],
            &[response("old", 10)],
            &[context("old", 10)],
            &[rag("old", 10)],
        )
        .unwrap();
        assert_eq!(stale_counts.transcript_segments, 0);
        assert_eq!(stale_counts.cue_responses, 0);
        assert_eq!(stale_counts.context_artifacts, 0);
        assert_eq!(stale_counts.rag_chunks, 0);

        let bundle = load_session(&pool, account_id, session_id)
            .unwrap()
            .expect("session remains visible");
        assert_eq!(bundle.session.title, "new");
        assert_eq!(bundle.session.answer_style.as_deref(), Some("new"));
        assert_eq!(bundle.transcript_segments[0].text, "new");
        assert_eq!(bundle.cue_responses[0].text, "new");
        assert_eq!(bundle.context_artifacts[0].title, "new");
        assert_eq!(bundle.context_artifacts[0].created_at_ms, 1);
        assert_eq!(bundle.context_artifacts[0].updated_at_ms, 20);

        let exact_retry_counts = upsert_batch(
            &pool,
            account_id,
            &[],
            &[],
            &[],
            &[context("same-revision-retry", 20)],
            &[],
        )
        .unwrap();
        assert_eq!(exact_retry_counts.context_artifacts, 0);
        let retried = load_session(&pool, account_id, session_id)
            .unwrap()
            .expect("session remains visible");
        assert_eq!(retried.context_artifacts[0].title, "new");
        let rag_text: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT text FROM cloud_rag_chunks WHERE account_id = ?1 AND chunk_id = ?2",
                rusqlite::params![account_id, "rag-versioned"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rag_text, "new");
    }

    #[test]
    fn tombstoned_session_does_not_resurrect_on_later_sync() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_delete";
        let session_id = uuid::Uuid::new_v4().to_string();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, "delete@example.com", "hash"],
            )
            .unwrap();

        let session = SyncSessionRecord {
            session_id: session_id.clone(),
            title: "Interview prep".into(),
            status: "active".into(),
            created_at_ms: 1,
            updated_at_ms: 10,
            last_active_at_ms: Some(10),
            answer_style: None,
            metadata: serde_json::json!({}),
            deleted_at_ms: None,
        };
        let response = SyncCueResponseRecord {
            response_id: "turn-delete-1".into(),
            session_id: session_id.clone(),
            kind: "answer".into(),
            text: "Real answer".into(),
            source_text: Some("Question".into()),
            ts_ms: 11,
            provider: Some("bluey_managed".into()),
            model: Some("balanced".into()),
            lane: Some("balanced".into()),
            task_type: Some("general".into()),
            cost_cents: None,
            balance_cents_after: None,
            cost_label: None,
            artifact_type: None,
            artifact_body: None,
            artifact_confidence: None,
            metadata: serde_json::json!({}),
        };
        upsert_batch(
            &pool,
            account_id,
            std::slice::from_ref(&session),
            &[SyncTranscriptSegment {
                segment_id: "segment-delete-1".into(),
                session_id: session_id.clone(),
                speaker: "user".into(),
                source: "microphone".into(),
                text: "Delete this transcript".into(),
                start_ms: None,
                end_ms: None,
                ts_ms: 10,
                is_final: true,
                metadata: serde_json::json!({}),
            }],
            std::slice::from_ref(&response),
            &[SyncContextArtifactRecord {
                artifact_id: "artifact-delete-1".into(),
                session_id: session_id.clone(),
                kind: "document".into(),
                title: "Delete this document".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: Some("private text".into()),
                created_at_ms: 10,
                updated_at_ms: 10,
                metadata: serde_json::json!({}),
            }],
            &[SyncRagChunkRecord {
                chunk_id: "rag-delete-1".into(),
                session_id: Some(session_id.clone()),
                source_kind: "document".into(),
                source_id: "artifact-delete-1".into(),
                chunk_index: 0,
                text: "Delete this indexed text".into(),
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: 10,
                metadata: serde_json::json!({}),
            }],
        )
        .unwrap();
        assert_eq!(list_sessions(&pool, account_id, 10).unwrap().len(), 1);

        tombstone_session(&pool, account_id, &session_id).unwrap();
        assert!(list_sessions(&pool, account_id, 10).unwrap().is_empty());
        let deleted_sessions = list_deleted_sessions(&pool, account_id, 10).unwrap();
        assert_eq!(deleted_sessions.len(), 1);
        assert_eq!(deleted_sessions[0].session_id, session_id);
        assert!(load_session(&pool, account_id, &session_id)
            .unwrap()
            .is_none());
        let conn = pool.get().unwrap();
        for table in [
            "cloud_transcript_segments",
            "cloud_cue_responses",
            "cloud_context_artifacts",
            "cloud_rag_chunks",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {table} WHERE account_id = ?1 AND session_id = ?2"
                    ),
                    rusqlite::params![account_id, session_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "{table} content must be purged on delete");
        }
        drop(conn);

        let stale_counts =
            upsert_batch(&pool, account_id, &[session], &[], &[response], &[], &[]).unwrap();
        assert_eq!(stale_counts.cue_responses, 0);
        assert!(
            list_sessions(&pool, account_id, 10).unwrap().is_empty(),
            "old desktop sync must not resurrect a deleted cloud session"
        );
        let retained_response_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM cloud_cue_responses
                 WHERE account_id = ?1 AND session_id = ?2",
                rusqlite::params![account_id, session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            retained_response_count, 0,
            "stale sync must not repopulate child data after deletion"
        );
        assert_eq!(
            list_deleted_sessions(&pool, account_id, 10).unwrap().len(),
            1
        );
    }

    #[test]
    fn artifact_upload_is_linked_and_delete_is_enqueued_with_session_tombstone() {
        use crate::db::object_uploads::{
            mark_upload_ready, reserve_upload, NewObjectUpload, ObjectKind, StorageScope,
        };
        use crate::object_storage::{sha256_hex, UploadLimits};

        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_object_delete";
        let session_id = uuid::Uuid::new_v4().to_string();
        let artifact_id = uuid::Uuid::new_v4().to_string();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, "objects@example.com", "hash"],
            )
            .unwrap();

        let created_at_ms = now_ms();
        let hash = sha256_hex("artifact bytes");
        let reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: artifact_id.clone(),
                session_id: None,
                storage_scope: StorageScope::Artifact,
                object_key: format!("objects/accounts/{account_id}/{artifact_id}/{hash}"),
                size_bytes: 14,
                sha256: hash,
                content_type: "text/plain".into(),
                expires_at_ms: created_at_ms + 86_400_000,
                metadata_json: serde_json::json!({}),
                now_ms: created_at_ms,
                limits: UploadLimits {
                    max_object_bytes: 100,
                    max_account_bytes: 1_000,
                    max_daily_bytes: 1_000,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        mark_upload_ready(&pool, &reservation.upload.id, created_at_ms + 1).unwrap();

        upsert_batch(
            &pool,
            account_id,
            &[SyncSessionRecord {
                session_id: session_id.clone(),
                title: "Object session".into(),
                status: "active".into(),
                created_at_ms,
                updated_at_ms: created_at_ms,
                last_active_at_ms: None,
                answer_style: None,
                metadata: serde_json::json!({}),
                deleted_at_ms: None,
            }],
            &[],
            &[],
            &[SyncContextArtifactRecord {
                artifact_id: artifact_id.clone(),
                session_id: session_id.clone(),
                kind: "document".into(),
                title: "Document".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: None,
                created_at_ms,
                updated_at_ms: created_at_ms,
                metadata: serde_json::json!({}),
            }],
            &[],
        )
        .unwrap();
        let linked_session: Option<String> = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT session_id FROM object_uploads WHERE id = ?1",
                rusqlite::params![reservation.upload.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked_session.as_deref(), Some(session_id.as_str()));

        tombstone_session(&pool, account_id, &session_id).unwrap();
        let lifecycle: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                rusqlite::params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(lifecycle, ("delete_pending".into(), "pending".into()));
    }
}
