//! Cloud sync + RAG persistence helpers.
//!
//! The SQLite path backs local/dev alpha installs, while the Postgres path is
//! the server-side sync and cloud RAG runtime target.

use std::collections::{BTreeSet, HashMap, HashSet};

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
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
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
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
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
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
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
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
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
    pub rag_count: i64,
    pub child_tombstone_count: i64,
    pub child_tombstone_updated_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudDeletedSession {
    pub session_id: String,
    pub deleted_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionPageCursor {
    pub updated_at_ms: i64,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct CloudSessionSummaryPage {
    pub sessions: Vec<CloudSessionSummary>,
    pub next_cursor: Option<SessionPageCursor>,
}

#[derive(Debug, Clone)]
pub struct CloudDeletedSessionPage {
    pub sessions: Vec<CloudDeletedSession>,
    pub next_cursor: Option<SessionPageCursor>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudChildTombstone {
    pub child_kind: String,
    pub child_id: String,
    pub session_id: String,
    pub deleted_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudSessionBundle {
    pub session: SyncSessionRecord,
    pub transcript_segments: Vec<SyncTranscriptSegment>,
    pub cue_responses: Vec<SyncCueResponseRecord>,
    pub context_artifacts: Vec<SyncContextArtifactRecord>,
    pub rag_chunks: Vec<SyncRagChunkRecord>,
    pub child_tombstones: Vec<CloudChildTombstone>,
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

#[derive(Debug, thiserror::Error)]
pub enum SyncWriteError {
    #[error("{entity} id {id} is already owned by another account")]
    CrossAccountIdentity { entity: &'static str, id: String },
    #[error("{entity} id {id} already belongs to session {existing_session_id}")]
    ParentMismatch {
        entity: &'static str,
        id: String,
        existing_session_id: String,
    },
    #[error("parent session {session_id} does not exist for this account")]
    MissingParent { session_id: String },
    #[error("attachment id {artifact_id} on response {response_id} does not belong to its parent session")]
    AttachmentMismatch {
        response_id: String,
        artifact_id: String,
    },
    #[error("response {response_id} has invalid attachment metadata")]
    InvalidAttachmentMetadata { response_id: String },
    #[error("duplicate {entity} id {id} appears in one sync batch")]
    DuplicateIdentity { entity: &'static str, id: String },
    #[error("session deletion must use the atomic session DELETE endpoint")]
    UnsupportedSessionTombstone,
}

#[derive(Debug, Clone, Copy)]
struct ChildIdentity<'a> {
    entity: &'static str,
    child_kind: &'static str,
    table: &'static str,
    id_column: &'static str,
    id: &'a str,
    session_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ChildTombstoneSource<'a> {
    source_kind: Option<&'a str>,
    source_id: Option<&'a str>,
    chunk_index: Option<i64>,
}

fn child_identities<'a>(
    transcript_segments: &'a [SyncTranscriptSegment],
    cue_responses: &'a [SyncCueResponseRecord],
    context_artifacts: &'a [SyncContextArtifactRecord],
    rag_chunks: &'a [SyncRagChunkRecord],
) -> Vec<ChildIdentity<'a>> {
    let mut identities = Vec::with_capacity(
        transcript_segments.len()
            + cue_responses.len()
            + context_artifacts.len()
            + rag_chunks.len(),
    );
    identities.extend(transcript_segments.iter().map(|record| ChildIdentity {
        entity: "transcript segment",
        child_kind: "transcript",
        table: "cloud_transcript_segments",
        id_column: "segment_id",
        id: &record.segment_id,
        session_id: Some(&record.session_id),
    }));
    identities.extend(cue_responses.iter().map(|record| ChildIdentity {
        entity: "response",
        child_kind: "response",
        table: "cloud_cue_responses",
        id_column: "response_id",
        id: &record.response_id,
        session_id: Some(&record.session_id),
    }));
    identities.extend(context_artifacts.iter().map(|record| ChildIdentity {
        entity: "context artifact",
        child_kind: "context",
        table: "cloud_context_artifacts",
        id_column: "artifact_id",
        id: &record.artifact_id,
        session_id: Some(&record.session_id),
    }));
    identities.extend(rag_chunks.iter().map(|record| ChildIdentity {
        entity: "RAG chunk",
        child_kind: "rag",
        table: "cloud_rag_chunks",
        id_column: "chunk_id",
        id: &record.chunk_id,
        session_id: record.session_id.as_deref(),
    }));
    identities
}

fn response_attachment_ids(record: &SyncCueResponseRecord) -> Result<Vec<&str>> {
    let Some(value) = record.metadata.get("attachment_ids") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let Some(values) = value.as_array() else {
        return Err(SyncWriteError::InvalidAttachmentMetadata {
            response_id: record.response_id.clone(),
        }
        .into());
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| {
                    SyncWriteError::InvalidAttachmentMetadata {
                        response_id: record.response_id.clone(),
                    }
                    .into()
                })
        })
        .collect()
}

fn validate_incoming_identities(
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<()> {
    let mut session_ids = HashSet::new();
    for record in sessions {
        if record.deleted_at_ms.is_some() {
            return Err(SyncWriteError::UnsupportedSessionTombstone.into());
        }
        if !session_ids.insert(record.session_id.as_str()) {
            return Err(SyncWriteError::DuplicateIdentity {
                entity: "session",
                id: record.session_id.clone(),
            }
            .into());
        }
    }

    let identities = child_identities(
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    );
    let mut child_ids = HashSet::new();
    for identity in identities {
        if !child_ids.insert((identity.entity, identity.id)) {
            return Err(SyncWriteError::DuplicateIdentity {
                entity: identity.entity,
                id: identity.id.to_string(),
            }
            .into());
        }
    }

    let mut canvas_ids = HashSet::new();
    for response in cue_responses {
        response_attachment_ids(response)?;
        let Some(canvas_id) = response
            .metadata
            .get("canvas_artifact_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
        else {
            continue;
        };
        if !canvas_ids.insert(canvas_id) {
            return Err(SyncWriteError::DuplicateIdentity {
                entity: "canvas artifact",
                id: canvas_id.to_string(),
            }
            .into());
        }
    }
    Ok(())
}

fn sync_lock_keys(
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for session in sessions {
        keys.insert(crate::db::object_uploads::session_advisory_lock_key(
            &session.session_id,
        ));
    }
    for identity in child_identities(
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    ) {
        if identity.entity == "context artifact" {
            keys.insert(crate::db::object_uploads::context_artifact_advisory_lock_key(identity.id));
        } else {
            keys.insert(format!("{}:{}", identity.entity, identity.id));
        }
        if let Some(session_id) = identity.session_id {
            keys.insert(crate::db::object_uploads::session_advisory_lock_key(
                session_id,
            ));
        }
    }
    for response in cue_responses {
        for artifact_id in response_attachment_ids(response)? {
            keys.insert(crate::db::object_uploads::context_artifact_advisory_lock_key(artifact_id));
        }
    }
    Ok(keys)
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
    validate_incoming_identities(
        sessions,
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    )?;
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

fn session_owned_by_account_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<bool> {
    let mut stmt = tx.prepare("SELECT account_id FROM cloud_sessions WHERE session_id = ?1")?;
    let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
    let mut owned = false;
    for row in rows {
        if row? == account_id {
            owned = true;
        } else {
            return Err(SyncWriteError::CrossAccountIdentity {
                entity: "session",
                id: session_id.to_string(),
            }
            .into());
        }
    }
    Ok(owned)
}

fn validate_child_identity_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    identity: ChildIdentity<'_>,
) -> Result<bool> {
    let sql = format!(
        "SELECT account_id, session_id FROM {} WHERE {} = ?1",
        identity.table, identity.id_column
    );
    let mut stmt = tx.prepare(&sql)?;
    let rows = stmt.query_map(params![identity.id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let expected_parent = identity.session_id.map(ToString::to_string);
    let mut matched = false;
    for row in rows {
        let (owner, existing_parent) = row?;
        if owner != account_id {
            return Err(SyncWriteError::CrossAccountIdentity {
                entity: identity.entity,
                id: identity.id.to_string(),
            }
            .into());
        }
        if existing_parent != expected_parent {
            return Err(SyncWriteError::ParentMismatch {
                entity: identity.entity,
                id: identity.id.to_string(),
                existing_session_id: existing_parent.unwrap_or_else(|| "<none>".to_string()),
            }
            .into());
        }
        matched = true;
    }
    let tombstone_parent = tx
        .query_row(
            "SELECT session_id
             FROM cloud_child_tombstones
             WHERE account_id = ?1 AND child_kind = ?2 AND child_id = ?3",
            params![account_id, identity.child_kind, identity.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing_session_id) =
        tombstone_parent.filter(|parent| expected_parent.as_ref() != Some(parent))
    {
        return Err(SyncWriteError::ParentMismatch {
            entity: identity.entity,
            id: identity.id.to_string(),
            existing_session_id,
        }
        .into());
    }
    Ok(matched)
}

fn validate_batch_ownership_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<()> {
    let identities = child_identities(
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    );
    let incoming_sessions = sessions
        .iter()
        .map(|record| record.session_id.as_str())
        .collect::<HashSet<_>>();
    let mut all_sessions = incoming_sessions.iter().copied().collect::<BTreeSet<_>>();
    for identity in &identities {
        if let Some(session_id) = identity.session_id {
            all_sessions.insert(session_id);
        }
    }

    for session_id in all_sessions {
        let owned = session_owned_by_account_sqlite_tx(tx, account_id, session_id)?;
        if !incoming_sessions.contains(session_id) && !owned {
            return Err(SyncWriteError::MissingParent {
                session_id: session_id.to_string(),
            }
            .into());
        }
    }

    for identity in identities {
        validate_child_identity_sqlite_tx(tx, account_id, identity)?;
    }

    let incoming_context = context_artifacts
        .iter()
        .map(|record| (record.artifact_id.as_str(), record.session_id.as_str()))
        .collect::<HashMap<_, _>>();
    for response in cue_responses {
        for artifact_id in response_attachment_ids(response)? {
            if let Some(parent) = incoming_context.get(artifact_id) {
                if *parent == response.session_id {
                    continue;
                }
                return Err(SyncWriteError::AttachmentMismatch {
                    response_id: response.response_id.clone(),
                    artifact_id: artifact_id.to_string(),
                }
                .into());
            }
            let matched = validate_child_identity_sqlite_tx(
                tx,
                account_id,
                ChildIdentity {
                    entity: "attachment",
                    child_kind: "context",
                    table: "cloud_context_artifacts",
                    id_column: "artifact_id",
                    id: artifact_id,
                    session_id: Some(&response.session_id),
                },
            )?;
            if !matched {
                return Err(SyncWriteError::AttachmentMismatch {
                    response_id: response.response_id.clone(),
                    artifact_id: artifact_id.to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
}

fn lock_sync_identities_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<()> {
    for key in sync_lock_keys(
        sessions,
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    )? {
        tx.query(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))",
            &[&key],
        )?;
    }
    Ok(())
}

fn session_owned_by_account_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    session_id: &str,
) -> Result<bool> {
    let rows = tx.query(
        "SELECT account_id FROM cloud_sessions WHERE session_id = $1",
        &[&session_id],
    )?;
    let mut owned = false;
    for row in rows {
        let owner: String = row.try_get(0)?;
        if owner == account_id {
            owned = true;
        } else {
            return Err(SyncWriteError::CrossAccountIdentity {
                entity: "session",
                id: session_id.to_string(),
            }
            .into());
        }
    }
    Ok(owned)
}

fn validate_child_identity_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    identity: ChildIdentity<'_>,
) -> Result<bool> {
    let sql = format!(
        "SELECT account_id, session_id FROM {} WHERE {} = $1",
        identity.table, identity.id_column
    );
    let rows = tx.query(&sql, &[&identity.id])?;
    let expected_parent = identity.session_id.map(ToString::to_string);
    let mut matched = false;
    for row in rows {
        let owner: String = row.try_get(0)?;
        let existing_parent: Option<String> = row.try_get(1)?;
        if owner != account_id {
            return Err(SyncWriteError::CrossAccountIdentity {
                entity: identity.entity,
                id: identity.id.to_string(),
            }
            .into());
        }
        if existing_parent != expected_parent {
            return Err(SyncWriteError::ParentMismatch {
                entity: identity.entity,
                id: identity.id.to_string(),
                existing_session_id: existing_parent.unwrap_or_else(|| "<none>".to_string()),
            }
            .into());
        }
        matched = true;
    }
    let tombstone_parent = tx
        .query_opt(
            "SELECT session_id
             FROM cloud_child_tombstones
             WHERE account_id = $1 AND child_kind = $2 AND child_id = $3",
            &[&account_id, &identity.child_kind, &identity.id],
        )?
        .map(|row| row.get::<_, String>(0));
    if let Some(existing_session_id) =
        tombstone_parent.filter(|parent| expected_parent.as_ref() != Some(parent))
    {
        return Err(SyncWriteError::ParentMismatch {
            entity: identity.entity,
            id: identity.id.to_string(),
            existing_session_id,
        }
        .into());
    }
    Ok(matched)
}

fn validate_batch_ownership_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    sessions: &[SyncSessionRecord],
    transcript_segments: &[SyncTranscriptSegment],
    cue_responses: &[SyncCueResponseRecord],
    context_artifacts: &[SyncContextArtifactRecord],
    rag_chunks: &[SyncRagChunkRecord],
) -> Result<()> {
    lock_sync_identities_postgres_tx(
        tx,
        sessions,
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    )?;
    let identities = child_identities(
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    );
    let incoming_sessions = sessions
        .iter()
        .map(|record| record.session_id.as_str())
        .collect::<HashSet<_>>();
    let mut all_sessions = incoming_sessions.iter().copied().collect::<BTreeSet<_>>();
    for identity in &identities {
        if let Some(session_id) = identity.session_id {
            all_sessions.insert(session_id);
        }
    }

    for session_id in all_sessions {
        let owned = session_owned_by_account_postgres_tx(tx, account_id, session_id)?;
        if !incoming_sessions.contains(session_id) && !owned {
            return Err(SyncWriteError::MissingParent {
                session_id: session_id.to_string(),
            }
            .into());
        }
    }

    for identity in identities {
        validate_child_identity_postgres_tx(tx, account_id, identity)?;
    }

    let incoming_context = context_artifacts
        .iter()
        .map(|record| (record.artifact_id.as_str(), record.session_id.as_str()))
        .collect::<HashMap<_, _>>();
    for response in cue_responses {
        for artifact_id in response_attachment_ids(response)? {
            if let Some(parent) = incoming_context.get(artifact_id) {
                if *parent == response.session_id {
                    continue;
                }
                return Err(SyncWriteError::AttachmentMismatch {
                    response_id: response.response_id.clone(),
                    artifact_id: artifact_id.to_string(),
                }
                .into());
            }
            let matched = validate_child_identity_postgres_tx(
                tx,
                account_id,
                ChildIdentity {
                    entity: "attachment",
                    child_kind: "context",
                    table: "cloud_context_artifacts",
                    id_column: "artifact_id",
                    id: artifact_id,
                    session_id: Some(&response.session_id),
                },
            )?;
            if !matched {
                return Err(SyncWriteError::AttachmentMismatch {
                    response_id: response.response_id.clone(),
                    artifact_id: artifact_id.to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
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
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .context("begin sync tx")?;
    let mut applied_transcript_segments = 0;
    let mut applied_cue_responses = 0;
    let mut applied_context_artifacts = 0;
    let mut applied_rag_chunks = 0;

    validate_batch_ownership_sqlite_tx(
        &tx,
        account_id,
        sessions,
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    )?;

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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_sqlite_tx(
                &tx,
                account_id,
                "transcript",
                &record.segment_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_transcript_segments += 1;
            }
            continue;
        }
        if !child_write_allowed_sqlite_tx(
            &tx,
            account_id,
            "transcript",
            &record.segment_id,
            record.ts_ms,
        )? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        applied_transcript_segments += tx.execute(
            "INSERT INTO cloud_transcript_segments (
                account_id, segment_id, session_id, speaker, source, text,
                start_ms, end_ms, ts_ms, is_final, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(account_id, segment_id) DO UPDATE SET
                speaker=excluded.speaker,
                source=excluded.source,
                text=excluded.text,
                start_ms=excluded.start_ms,
                end_ms=excluded.end_ms,
                ts_ms=excluded.ts_ms,
                is_final=excluded.is_final,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_transcript_segments.ts_ms
               AND cloud_transcript_segments.session_id = excluded.session_id",
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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_sqlite_tx(
                &tx,
                account_id,
                "response",
                &record.response_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_cue_responses += 1;
            }
            continue;
        }
        if !child_write_allowed_sqlite_tx(
            &tx,
            account_id,
            "response",
            &record.response_id,
            record.ts_ms,
        )? {
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
             WHERE excluded.ts_ms >= cloud_cue_responses.ts_ms
               AND cloud_cue_responses.session_id = excluded.session_id",
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
        let updated_at_ms = record.updated_at_ms.max(record.created_at_ms);
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_sqlite_tx(
                &tx,
                account_id,
                "context",
                &record.artifact_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_context_artifacts += 1;
            }
            continue;
        }
        if !child_write_allowed_sqlite_tx(
            &tx,
            account_id,
            "context",
            &record.artifact_id,
            updated_at_ms,
        )? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let affected = tx.execute(
            "INSERT INTO cloud_context_artifacts (
                account_id, artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(account_id, artifact_id) DO UPDATE SET
                kind=excluded.kind,
                title=excluded.title,
                note=excluded.note,
                source_uri=excluded.source_uri,
                content_hash=excluded.content_hash,
                text_preview=excluded.text_preview,
                created_at_ms=excluded.created_at_ms,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms > cloud_context_artifacts.updated_at_ms
               AND cloud_context_artifacts.session_id = excluded.session_id",
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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            let Some(session_id) = record.session_id.as_deref() else {
                continue;
            };
            if apply_child_tombstone_sqlite_tx(
                &tx,
                account_id,
                "rag",
                &record.chunk_id,
                session_id,
                deleted_at_ms,
                ChildTombstoneSource {
                    source_kind: Some(&record.source_kind),
                    source_id: Some(&record.source_id),
                    chunk_index: Some(record.chunk_index),
                },
            )? {
                applied_rag_chunks += 1;
            }
            continue;
        }
        if record.source_kind == "context"
            && context_artifact_is_final_deleted_sqlite_tx(&tx, account_id, &record.source_id)?
        {
            continue;
        }
        if !child_write_allowed_sqlite_tx(
            &tx,
            account_id,
            "rag",
            &record.chunk_id,
            record.updated_at_ms,
        )? {
            continue;
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
             WHERE excluded.updated_at_ms >= cloud_rag_chunks.updated_at_ms
               AND cloud_rag_chunks.session_id IS excluded.session_id",
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

    validate_batch_ownership_postgres_tx(
        &mut tx,
        account_id,
        sessions,
        transcript_segments,
        cue_responses,
        context_artifacts,
        rag_chunks,
    )?;

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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_postgres_tx(
                &mut tx,
                account_id,
                "transcript",
                &record.segment_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_transcript_segments += 1;
            }
            continue;
        }
        if !child_write_allowed_postgres_tx(
            &mut tx,
            account_id,
            "transcript",
            &record.segment_id,
            record.ts_ms,
        )? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let is_final = if record.is_final { 1_i32 } else { 0_i32 };
        let speaker = db_text(&record.speaker);
        let source = db_text(&record.source);
        let text = db_text(&record.text);
        applied_transcript_segments += tx
            .execute(
                "INSERT INTO cloud_transcript_segments (
                account_id, segment_id, session_id, speaker, source, text,
                start_ms, end_ms, ts_ms, is_final, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT(account_id, segment_id) DO UPDATE SET
                speaker=excluded.speaker,
                source=excluded.source,
                text=excluded.text,
                start_ms=excluded.start_ms,
                end_ms=excluded.end_ms,
                ts_ms=excluded.ts_ms,
                is_final=excluded.is_final,
                metadata_json=excluded.metadata_json
             WHERE excluded.ts_ms >= cloud_transcript_segments.ts_ms
               AND cloud_transcript_segments.session_id = excluded.session_id",
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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_postgres_tx(
                &mut tx,
                account_id,
                "response",
                &record.response_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_cue_responses += 1;
            }
            continue;
        }
        if !child_write_allowed_postgres_tx(
            &mut tx,
            account_id,
            "response",
            &record.response_id,
            record.ts_ms,
        )? {
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
             WHERE excluded.ts_ms >= cloud_cue_responses.ts_ms
               AND cloud_cue_responses.session_id = excluded.session_id",
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
        let updated_at_ms = record.updated_at_ms.max(record.created_at_ms);
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            if apply_child_tombstone_postgres_tx(
                &mut tx,
                account_id,
                "context",
                &record.artifact_id,
                &record.session_id,
                deleted_at_ms,
                ChildTombstoneSource::default(),
            )? {
                applied_context_artifacts += 1;
            }
            continue;
        }
        if !child_write_allowed_postgres_tx(
            &mut tx,
            account_id,
            "context",
            &record.artifact_id,
            updated_at_ms,
        )? {
            continue;
        }
        let metadata = serde_json::to_string(&record.metadata)?;
        let kind = db_text(&record.kind);
        let title = db_text(&record.title);
        let note = db_opt_text(&record.note);
        let source_uri = db_opt_text(&record.source_uri);
        let content_hash = db_opt_text(&record.content_hash);
        let text_preview = db_opt_text(&record.text_preview);
        let affected = tx
            .execute(
                "INSERT INTO cloud_context_artifacts (
                account_id, artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, updated_at_ms, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT(account_id, artifact_id) DO UPDATE SET
                kind=excluded.kind,
                title=excluded.title,
                note=excluded.note,
                source_uri=excluded.source_uri,
                content_hash=excluded.content_hash,
                text_preview=excluded.text_preview,
                created_at_ms=excluded.created_at_ms,
                updated_at_ms=excluded.updated_at_ms,
                metadata_json=excluded.metadata_json
             WHERE excluded.updated_at_ms > cloud_context_artifacts.updated_at_ms
               AND cloud_context_artifacts.session_id = excluded.session_id",
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
        if let Some(deleted_at_ms) = record.deleted_at_ms {
            let Some(session_id) = record.session_id.as_deref() else {
                continue;
            };
            if apply_child_tombstone_postgres_tx(
                &mut tx,
                account_id,
                "rag",
                &record.chunk_id,
                session_id,
                deleted_at_ms,
                ChildTombstoneSource {
                    source_kind: Some(&record.source_kind),
                    source_id: Some(&record.source_id),
                    chunk_index: Some(record.chunk_index),
                },
            )? {
                applied_rag_chunks += 1;
            }
            continue;
        }
        if record.source_kind == "context"
            && context_artifact_is_final_deleted_postgres_tx(
                &mut tx,
                account_id,
                &record.source_id,
            )?
        {
            continue;
        }
        if !child_write_allowed_postgres_tx(
            &mut tx,
            account_id,
            "rag",
            &record.chunk_id,
            record.updated_at_ms,
        )? {
            continue;
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
        applied_rag_chunks += tx
            .execute(
                "INSERT INTO cloud_rag_chunks (
                account_id, chunk_id, session_id, source_kind, source_id, chunk_index,
                text, embedding_json, embedding, embedding_model, token_count, content_hash,
                updated_at_ms, metadata_json
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::vector, $10, $11, $12, $13, $14)
             ON CONFLICT(account_id, chunk_id) DO UPDATE SET
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
             WHERE excluded.updated_at_ms >= cloud_rag_chunks.updated_at_ms
               AND cloud_rag_chunks.session_id IS NOT DISTINCT FROM excluded.session_id",
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
        DbPool::Sqlite(_) => list_sessions_sqlite(pool, account_id, limit, None),
        DbPool::Postgres(_) => list_sessions_postgres(pool, account_id, limit, None),
    })
}

pub fn list_sessions_page(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
    cursor: Option<&SessionPageCursor>,
) -> Result<CloudSessionSummaryPage> {
    let limit = limit.max(1);
    let mut sessions = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            list_sessions_sqlite(pool, account_id, limit.saturating_add(1), cursor)
        }
        DbPool::Postgres(_) => {
            list_sessions_postgres(pool, account_id, limit.saturating_add(1), cursor)
        }
    })?;
    let has_more = sessions.len() > limit as usize;
    if has_more {
        sessions.truncate(limit as usize);
    }
    let next_cursor = has_more.then(|| {
        let last = sessions
            .last()
            .expect("a page with an extra row has a returned row");
        SessionPageCursor {
            updated_at_ms: last.updated_at_ms,
            session_id: last.session_id.clone(),
        }
    });
    Ok(CloudSessionSummaryPage {
        sessions,
        next_cursor,
    })
}

pub fn list_deleted_sessions(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<CloudDeletedSession>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => list_deleted_sessions_sqlite(pool, account_id, limit, None),
        DbPool::Postgres(_) => list_deleted_sessions_postgres(pool, account_id, limit, None),
    })
}

pub fn list_deleted_sessions_page(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
    cursor: Option<&SessionPageCursor>,
) -> Result<CloudDeletedSessionPage> {
    let limit = limit.max(1);
    let mut sessions = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            list_deleted_sessions_sqlite(pool, account_id, limit.saturating_add(1), cursor)
        }
        DbPool::Postgres(_) => {
            list_deleted_sessions_postgres(pool, account_id, limit.saturating_add(1), cursor)
        }
    })?;
    let has_more = sessions.len() > limit as usize;
    if has_more {
        sessions.truncate(limit as usize);
    }
    let next_cursor = has_more.then(|| {
        let last = sessions
            .last()
            .expect("a page with an extra row has a returned row");
        SessionPageCursor {
            updated_at_ms: last.deleted_at_ms,
            session_id: last.session_id.clone(),
        }
    });
    Ok(CloudDeletedSessionPage {
        sessions,
        next_cursor,
    })
}

fn apply_child_tombstone_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    child_kind: &str,
    child_id: &str,
    session_id: &str,
    deleted_at_ms: i64,
    source: ChildTombstoneSource<'_>,
) -> Result<bool> {
    validate_child_tombstone_source(child_kind, source)?;
    let changed = tx.execute(
        "INSERT INTO cloud_child_tombstones (
            account_id, child_kind, child_id, session_id, deleted_at_ms,
            source_kind, source_id, chunk_index
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(account_id, child_kind, child_id) DO UPDATE SET
            session_id=excluded.session_id,
            deleted_at_ms=MAX(
                cloud_child_tombstones.deleted_at_ms,
                excluded.deleted_at_ms
            ),
            source_kind=COALESCE(
                cloud_child_tombstones.source_kind,
                excluded.source_kind
            ),
            source_id=COALESCE(cloud_child_tombstones.source_id, excluded.source_id),
            chunk_index=COALESCE(
                cloud_child_tombstones.chunk_index,
                excluded.chunk_index
            )
         WHERE excluded.deleted_at_ms > cloud_child_tombstones.deleted_at_ms
            OR (
                excluded.deleted_at_ms = cloud_child_tombstones.deleted_at_ms
                AND (
                    (cloud_child_tombstones.source_kind IS NULL
                        AND excluded.source_kind IS NOT NULL)
                    OR (cloud_child_tombstones.source_id IS NULL
                        AND excluded.source_id IS NOT NULL)
                    OR (cloud_child_tombstones.chunk_index IS NULL
                        AND excluded.chunk_index IS NOT NULL)
                )
            )",
        params![
            account_id,
            child_kind,
            child_id,
            session_id,
            deleted_at_ms,
            source.source_kind,
            source.source_id,
            source.chunk_index,
        ],
    )?;
    let deleted = match child_kind {
        "transcript" => tx.execute(
            "DELETE FROM cloud_transcript_segments
             WHERE account_id = ?1
               AND segment_id = ?2
               AND session_id = ?3
               AND ts_ms <= ?4",
            params![account_id, child_id, session_id, deleted_at_ms],
        )?,
        "response" => tx.execute(
            "DELETE FROM cloud_cue_responses
             WHERE account_id = ?1
               AND response_id = ?2
               AND session_id = ?3
               AND ts_ms <= ?4",
            params![account_id, child_id, session_id, deleted_at_ms],
        )?,
        "context" => {
            let deleted = tx.execute(
                "DELETE FROM cloud_context_artifacts
                 WHERE account_id = ?1
                   AND artifact_id = ?2
                   AND session_id = ?3
                   AND updated_at_ms <= ?4",
                params![account_id, child_id, session_id, deleted_at_ms],
            )?;
            let surviving = tx
                .query_row(
                    "SELECT 1
                       FROM cloud_context_artifacts
                      WHERE account_id = ?1 AND artifact_id = ?2 AND session_id = ?3",
                    params![account_id, child_id, session_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !surviving && (changed > 0 || deleted > 0) {
                crate::db::object_uploads::schedule_artifact_cleanup_sqlite_tx(
                    tx,
                    account_id,
                    child_id,
                    deleted_at_ms,
                )?;
            }
            deleted
        }
        "rag" => tx.execute(
            "DELETE FROM cloud_rag_chunks
             WHERE account_id = ?1
               AND chunk_id = ?2
               AND session_id = ?3
               AND updated_at_ms <= ?4",
            params![account_id, child_id, session_id, deleted_at_ms],
        )?,
        _ => anyhow::bail!("unsupported cloud child tombstone kind"),
    };
    Ok(changed > 0 || deleted > 0)
}

fn child_write_allowed_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    child_kind: &str,
    child_id: &str,
    updated_at_ms: i64,
) -> Result<bool> {
    if child_kind == "context"
        && context_artifact_is_final_deleted_sqlite_tx(tx, account_id, child_id)?
    {
        return Ok(false);
    }
    let deleted_at_ms = tx
        .query_row(
            "SELECT deleted_at_ms
             FROM cloud_child_tombstones
             WHERE account_id = ?1 AND child_kind = ?2 AND child_id = ?3",
            params![account_id, child_kind, child_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(deleted_at_ms) = deleted_at_ms else {
        return Ok(true);
    };
    if child_kind == "context" {
        return Ok(false);
    }
    if deleted_at_ms >= updated_at_ms {
        return Ok(false);
    }
    tx.execute(
        "DELETE FROM cloud_child_tombstones
         WHERE account_id = ?1 AND child_kind = ?2 AND child_id = ?3",
        params![account_id, child_kind, child_id],
    )?;
    Ok(true)
}

fn context_artifact_is_final_deleted_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    artifact_id: &str,
) -> Result<bool> {
    let tombstoned = tx
        .query_row(
            "SELECT 1
               FROM cloud_child_tombstones
              WHERE account_id = ?1 AND child_kind = 'context' AND child_id = ?2",
            params![account_id, artifact_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if tombstoned {
        return Ok(true);
    }
    Ok(tx
        .query_row(
            "SELECT 1
               FROM object_uploads
              WHERE account_id = ?1
                AND object_kind = 'artifact'
                AND logical_id = ?2
                AND state IN ('delete_pending', 'deleted')",
            params![account_id, artifact_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn apply_child_tombstone_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    child_kind: &str,
    child_id: &str,
    session_id: &str,
    deleted_at_ms: i64,
    source: ChildTombstoneSource<'_>,
) -> Result<bool> {
    validate_child_tombstone_source(child_kind, source)?;
    let source_kind = source.source_kind.map(db_text);
    let source_id = source.source_id.map(db_text);
    let changed = tx.execute(
        "INSERT INTO cloud_child_tombstones (
            account_id, child_kind, child_id, session_id, deleted_at_ms,
            source_kind, source_id, chunk_index
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT(account_id, child_kind, child_id) DO UPDATE SET
            session_id=excluded.session_id,
            deleted_at_ms=GREATEST(
                cloud_child_tombstones.deleted_at_ms,
                excluded.deleted_at_ms
            ),
            source_kind=COALESCE(
                cloud_child_tombstones.source_kind,
                excluded.source_kind
            ),
            source_id=COALESCE(cloud_child_tombstones.source_id, excluded.source_id),
            chunk_index=COALESCE(
                cloud_child_tombstones.chunk_index,
                excluded.chunk_index
            )
         WHERE excluded.deleted_at_ms > cloud_child_tombstones.deleted_at_ms
            OR (
                excluded.deleted_at_ms = cloud_child_tombstones.deleted_at_ms
                AND (
                    (cloud_child_tombstones.source_kind IS NULL
                        AND excluded.source_kind IS NOT NULL)
                    OR (cloud_child_tombstones.source_id IS NULL
                        AND excluded.source_id IS NOT NULL)
                    OR (cloud_child_tombstones.chunk_index IS NULL
                        AND excluded.chunk_index IS NOT NULL)
                )
            )",
        &[
            &account_id,
            &child_kind,
            &child_id,
            &session_id,
            &deleted_at_ms,
            &source_kind,
            &source_id,
            &source.chunk_index,
        ],
    )?;
    let deleted = match child_kind {
        "transcript" => tx.execute(
            "DELETE FROM cloud_transcript_segments
             WHERE account_id = $1
               AND segment_id = $2
               AND session_id = $3
               AND ts_ms <= $4",
            &[&account_id, &child_id, &session_id, &deleted_at_ms],
        )?,
        "response" => tx.execute(
            "DELETE FROM cloud_cue_responses
             WHERE account_id = $1
               AND response_id = $2
               AND session_id = $3
               AND ts_ms <= $4",
            &[&account_id, &child_id, &session_id, &deleted_at_ms],
        )?,
        "context" => {
            let deleted = tx.execute(
                "DELETE FROM cloud_context_artifacts
                 WHERE account_id = $1
                   AND artifact_id = $2
                   AND session_id = $3
                   AND updated_at_ms <= $4",
                &[&account_id, &child_id, &session_id, &deleted_at_ms],
            )?;
            let surviving = tx
                .query_opt(
                    "SELECT 1
                       FROM cloud_context_artifacts
                      WHERE account_id = $1 AND artifact_id = $2 AND session_id = $3",
                    &[&account_id, &child_id, &session_id],
                )?
                .is_some();
            if !surviving && (changed > 0 || deleted > 0) {
                crate::db::object_uploads::schedule_artifact_cleanup_postgres_tx(
                    tx,
                    account_id,
                    child_id,
                    deleted_at_ms,
                )?;
            }
            deleted
        }
        "rag" => tx.execute(
            "DELETE FROM cloud_rag_chunks
             WHERE account_id = $1
               AND chunk_id = $2
               AND session_id = $3
               AND updated_at_ms <= $4",
            &[&account_id, &child_id, &session_id, &deleted_at_ms],
        )?,
        _ => anyhow::bail!("unsupported cloud child tombstone kind"),
    };
    Ok(changed > 0 || deleted > 0)
}

fn validate_child_tombstone_source(
    child_kind: &str,
    source: ChildTombstoneSource<'_>,
) -> Result<()> {
    if child_kind == "rag" {
        if source
            .source_kind
            .is_none_or(|value| value.trim().is_empty())
            || source.source_id.is_none_or(|value| value.trim().is_empty())
            || source.chunk_index.is_none()
        {
            anyhow::bail!("RAG child tombstone is missing source provenance");
        }
    } else if source.source_kind.is_some()
        || source.source_id.is_some()
        || source.chunk_index.is_some()
    {
        anyhow::bail!("non-RAG child tombstone has unexpected source provenance");
    }
    Ok(())
}

fn child_write_allowed_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    child_kind: &str,
    child_id: &str,
    updated_at_ms: i64,
) -> Result<bool> {
    if child_kind == "context"
        && context_artifact_is_final_deleted_postgres_tx(tx, account_id, child_id)?
    {
        return Ok(false);
    }
    let deleted_at_ms = tx
        .query_opt(
            "SELECT deleted_at_ms
             FROM cloud_child_tombstones
             WHERE account_id = $1 AND child_kind = $2 AND child_id = $3",
            &[&account_id, &child_kind, &child_id],
        )?
        .map(|row| row.get::<_, i64>(0));
    let Some(deleted_at_ms) = deleted_at_ms else {
        return Ok(true);
    };
    if child_kind == "context" {
        return Ok(false);
    }
    if deleted_at_ms >= updated_at_ms {
        return Ok(false);
    }
    tx.execute(
        "DELETE FROM cloud_child_tombstones
         WHERE account_id = $1 AND child_kind = $2 AND child_id = $3",
        &[&account_id, &child_kind, &child_id],
    )?;
    Ok(true)
}

fn context_artifact_is_final_deleted_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    artifact_id: &str,
) -> Result<bool> {
    let tombstoned = tx
        .query_opt(
            "SELECT 1
               FROM cloud_child_tombstones
              WHERE account_id = $1 AND child_kind = 'context' AND child_id = $2",
            &[&account_id, &artifact_id],
        )?
        .is_some();
    if tombstoned {
        return Ok(true);
    }
    Ok(tx
        .query_opt(
            "SELECT 1
               FROM object_uploads
              WHERE account_id = $1
                AND object_kind = 'artifact'
                AND logical_id = $2
                AND state IN ('delete_pending', 'deleted')",
            &[&account_id, &artifact_id],
        )?
        .is_some())
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
    cursor: Option<&SessionPageCursor>,
) -> Result<Vec<CloudDeletedSession>> {
    let conn = pool.get().context("get db conn")?;
    let cursor_updated_at_ms = cursor.map(|cursor| cursor.updated_at_ms);
    let cursor_session_id = cursor.map(|cursor| cursor.session_id.as_str());
    let mut stmt = conn.prepare(
        "WITH deleted_sessions AS (
            SELECT session_id,
                   COALESCE(deleted_at_ms, updated_at_ms) AS deletion_updated_at_ms,
                   updated_at_ms
            FROM cloud_sessions
            WHERE account_id = ?1 AND deleted_at_ms IS NOT NULL
         )
         SELECT session_id, deletion_updated_at_ms, updated_at_ms
         FROM deleted_sessions
         WHERE ?2 IS NULL
            OR deletion_updated_at_ms < ?2
            OR (deletion_updated_at_ms = ?2 AND session_id < ?3)
         ORDER BY deletion_updated_at_ms DESC, session_id DESC
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![account_id, cursor_updated_at_ms, cursor_session_id, limit],
        |row| {
            Ok(CloudDeletedSession {
                session_id: row.get(0)?,
                deleted_at_ms: row.get(1)?,
                updated_at_ms: row.get(2)?,
            })
        },
    )?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn list_deleted_sessions_postgres(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
    cursor: Option<&SessionPageCursor>,
) -> Result<Vec<CloudDeletedSession>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let cursor_updated_at_ms = cursor.map(|cursor| cursor.updated_at_ms);
    let cursor_session_id = cursor.map(|cursor| cursor.session_id.as_str());
    let rows = conn.query(
        "WITH deleted_sessions AS (
            SELECT session_id,
                   COALESCE(deleted_at_ms, updated_at_ms) AS deletion_updated_at_ms,
                   updated_at_ms
            FROM cloud_sessions
            WHERE account_id = $1 AND deleted_at_ms IS NOT NULL
         )
         SELECT session_id, deletion_updated_at_ms, updated_at_ms
         FROM deleted_sessions
         WHERE $2::bigint IS NULL
            OR deletion_updated_at_ms < $2
            OR (deletion_updated_at_ms = $2 AND session_id < $3::text)
         ORDER BY deletion_updated_at_ms DESC, session_id DESC
         LIMIT $4",
        &[
            &account_id,
            &cursor_updated_at_ms,
            &cursor_session_id,
            &limit,
        ],
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
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .context("begin session tombstone tx")?;
    session_owned_by_account_sqlite_tx(&tx, account_id, session_id)?;
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
    let lock_key = crate::db::object_uploads::session_advisory_lock_key(session_id);
    tx.query(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))",
        &[&lock_key],
    )?;
    session_owned_by_account_postgres_tx(&mut tx, account_id, session_id)?;
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
    cursor: Option<&SessionPageCursor>,
) -> Result<Vec<CloudSessionSummary>> {
    let conn = pool.get().context("get db conn")?;
    let cursor_updated_at_ms = cursor.map(|cursor| cursor.updated_at_ms);
    let cursor_session_id = cursor.map(|cursor| cursor.session_id.as_str());
    let mut stmt = conn.prepare(
        "WITH session_summaries AS (
            SELECT s.session_id, s.title, s.status,
                MAX(
                    s.updated_at_ms,
                    COALESCE((SELECT MAX(t.ts_ms) FROM cloud_transcript_segments t
                        WHERE t.account_id = s.account_id AND t.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(r.ts_ms) FROM cloud_cue_responses r
                        WHERE r.account_id = s.account_id AND r.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(c.updated_at_ms) FROM cloud_context_artifacts c
                        WHERE c.account_id = s.account_id AND c.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(g.updated_at_ms) FROM cloud_rag_chunks g
                        WHERE g.account_id = s.account_id AND g.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(d.deleted_at_ms) FROM cloud_child_tombstones d
                        WHERE d.account_id = s.account_id AND d.session_id = s.session_id), 0)
                ) AS content_updated_at_ms,
                s.last_active_at_ms,
                s.answer_style,
                (SELECT COUNT(*) FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                    AS transcript_count,
                (SELECT COUNT(*) FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                    AS response_count,
                (SELECT COUNT(*) FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
                    AS context_count,
                (SELECT COUNT(*) FROM cloud_rag_chunks g
                    WHERE g.account_id = s.account_id AND g.session_id = s.session_id)
                    AS rag_count,
                (SELECT COUNT(*) FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
                    AS child_tombstone_count,
                (SELECT MAX(d.deleted_at_ms) FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
                    AS child_tombstone_updated_at_ms
         FROM cloud_sessions s
         WHERE s.account_id = ?1 AND s.deleted_at_ms IS NULL
           AND (
                EXISTS (SELECT 1 FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_rag_chunks g
                    WHERE g.account_id = s.account_id AND g.session_id = s.session_id
                      AND TRIM(g.text) <> '')
                OR EXISTS (SELECT 1 FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
           )
         )
         SELECT session_id, title, status, content_updated_at_ms,
                last_active_at_ms, answer_style, transcript_count, response_count, context_count,
                rag_count, child_tombstone_count, child_tombstone_updated_at_ms
         FROM session_summaries
         WHERE ?2 IS NULL
            OR content_updated_at_ms < ?2
            OR (content_updated_at_ms = ?2 AND session_id < ?3)
         ORDER BY content_updated_at_ms DESC, session_id DESC
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![account_id, cursor_updated_at_ms, cursor_session_id, limit],
        |row| {
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
                rag_count: row.get(9)?,
                child_tombstone_count: row.get(10)?,
                child_tombstone_updated_at_ms: row.get(11)?,
            })
        },
    )?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn list_sessions_postgres(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
    cursor: Option<&SessionPageCursor>,
) -> Result<Vec<CloudSessionSummary>> {
    let mut conn = pool.get_pg().context("get postgres db conn")?;
    let cursor_updated_at_ms = cursor.map(|cursor| cursor.updated_at_ms);
    let cursor_session_id = cursor.map(|cursor| cursor.session_id.as_str());
    let rows = conn.query(
        "WITH session_summaries AS (
            SELECT s.session_id, s.title, s.status,
                GREATEST(
                    s.updated_at_ms,
                    COALESCE((SELECT MAX(t.ts_ms) FROM cloud_transcript_segments t
                        WHERE t.account_id = s.account_id AND t.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(r.ts_ms) FROM cloud_cue_responses r
                        WHERE r.account_id = s.account_id AND r.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(c.updated_at_ms) FROM cloud_context_artifacts c
                        WHERE c.account_id = s.account_id AND c.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(g.updated_at_ms) FROM cloud_rag_chunks g
                        WHERE g.account_id = s.account_id AND g.session_id = s.session_id), 0),
                    COALESCE((SELECT MAX(d.deleted_at_ms) FROM cloud_child_tombstones d
                        WHERE d.account_id = s.account_id AND d.session_id = s.session_id), 0)
                ) AS content_updated_at_ms,
                s.last_active_at_ms,
                s.answer_style,
                (SELECT COUNT(*)::bigint FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                    AS transcript_count,
                (SELECT COUNT(*)::bigint FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                    AS response_count,
                (SELECT COUNT(*)::bigint FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
                    AS context_count,
                (SELECT COUNT(*)::bigint FROM cloud_rag_chunks g
                    WHERE g.account_id = s.account_id AND g.session_id = s.session_id)
                    AS rag_count,
                (SELECT COUNT(*)::bigint FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
                    AS child_tombstone_count,
                (SELECT MAX(d.deleted_at_ms) FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
                    AS child_tombstone_updated_at_ms
         FROM cloud_sessions s
         WHERE s.account_id = $1 AND s.deleted_at_ms IS NULL
           AND (
                EXISTS (SELECT 1 FROM cloud_transcript_segments t
                    WHERE t.account_id = s.account_id AND t.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_cue_responses r
                    WHERE r.account_id = s.account_id AND r.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_context_artifacts c
                    WHERE c.account_id = s.account_id AND c.session_id = s.session_id)
                OR EXISTS (SELECT 1 FROM cloud_rag_chunks g
                    WHERE g.account_id = s.account_id AND g.session_id = s.session_id
                      AND BTRIM(g.text) <> '')
                OR EXISTS (SELECT 1 FROM cloud_child_tombstones d
                    WHERE d.account_id = s.account_id AND d.session_id = s.session_id)
           )
         )
         SELECT session_id, title, status, content_updated_at_ms,
                last_active_at_ms, answer_style, transcript_count, response_count, context_count,
                rag_count, child_tombstone_count, child_tombstone_updated_at_ms
         FROM session_summaries
         WHERE $2::bigint IS NULL
            OR content_updated_at_ms < $2
            OR (content_updated_at_ms = $2 AND session_id < $3::text)
         ORDER BY content_updated_at_ms DESC, session_id DESC
         LIMIT $4",
        &[
            &account_id,
            &cursor_updated_at_ms,
            &cursor_session_id,
            &limit,
        ],
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

/// Return whether an undeleted cloud session belongs to this account without
/// loading its private transcript, answer, context, or RAG children. This is a
/// cheap route-level indistinguishability check; write paths must still repeat
/// ownership checks inside their atomic transaction.
pub fn live_session_exists(pool: &DbPool, account_id: &str, session_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get().context("get db conn")?;
            Ok(conn
                .query_row(
                    "SELECT 1 FROM cloud_sessions
                      WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NULL",
                    params![account_id, session_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().context("get postgres db conn")?;
            Ok(conn
                .query_opt(
                    "SELECT 1 FROM cloud_sessions
                      WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NULL",
                    &[&account_id, &session_id],
                )?
                .is_some())
        }
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
        rag_chunks: load_rag_chunks(&conn, account_id, session_id)?,
        child_tombstones: load_child_tombstones(&conn, account_id, session_id)?,
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
        rag_chunks: load_rag_chunks_pg(&mut conn, account_id, session_id)?,
        child_tombstones: load_child_tombstones_pg(&mut conn, account_id, session_id)?,
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
                deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
            metadata: parse_json(&metadata),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn load_rag_chunks(
    conn: &rusqlite::Connection,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncRagChunkRecord>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, session_id, source_kind, source_id, chunk_index, text,
                token_count, content_hash, updated_at_ms, metadata_json
         FROM cloud_rag_chunks
         WHERE account_id = ?1 AND session_id = ?2
         ORDER BY updated_at_ms ASC, chunk_id ASC",
    )?;
    let rows = stmt.query_map(params![account_id, session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, String>(9)?,
        ))
    })?;
    let mut records = Vec::new();
    for row in rows {
        let (
            chunk_id,
            session_id,
            source_kind,
            source_id,
            chunk_index,
            text,
            token_count,
            content_hash,
            updated_at_ms,
            metadata,
        ) = row?;
        records.push(SyncRagChunkRecord {
            chunk_id,
            session_id,
            source_kind,
            source_id,
            chunk_index,
            text,
            // Embeddings are provider-derived server material. The desktop
            // reconstructs its own local index from the user-owned text.
            embedding: None,
            embedding_model: None,
            token_count,
            content_hash,
            updated_at_ms,
            deleted_at_ms: None,
            metadata: parse_json(&metadata),
        });
    }
    Ok(records)
}

fn load_child_tombstones(
    conn: &rusqlite::Connection,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<CloudChildTombstone>> {
    let mut stmt = conn.prepare(
        "SELECT child_kind, child_id, session_id, deleted_at_ms,
                source_kind, source_id, chunk_index
         FROM cloud_child_tombstones
         WHERE account_id = ?1 AND session_id = ?2
         ORDER BY deleted_at_ms ASC, child_kind ASC, child_id ASC",
    )?;
    let rows = stmt.query_map(params![account_id, session_id], |row| {
        Ok(CloudChildTombstone {
            child_kind: row.get(0)?,
            child_id: row.get(1)?,
            session_id: row.get(2)?,
            deleted_at_ms: row.get(3)?,
            source_kind: row.get(4)?,
            source_id: row.get(5)?,
            chunk_index: row.get(6)?,
        })
    })?;
    let records = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)?;
    validate_loaded_child_tombstones(records, session_id)
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
        rag_count: row.try_get(9)?,
        child_tombstone_count: row.try_get(10)?,
        child_tombstone_updated_at_ms: row.try_get(11)?,
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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
                metadata: parse_json(&metadata),
            })
        })
        .collect()
}

fn load_rag_chunks_pg(
    conn: &mut Client,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<SyncRagChunkRecord>> {
    let rows = conn.query(
        "SELECT chunk_id, session_id, source_kind, source_id, chunk_index, text,
                token_count, content_hash, updated_at_ms, metadata_json
         FROM cloud_rag_chunks
         WHERE account_id = $1 AND session_id = $2
         ORDER BY updated_at_ms ASC, chunk_id ASC",
        &[&account_id, &session_id],
    )?;
    rows.into_iter()
        .map(|row| {
            let chunk_id: String = row.try_get(0)?;
            let chunk_index: i32 = row.try_get(4)?;
            let token_count: Option<i64> = row.try_get(6)?;
            let metadata: String = row.try_get(9)?;
            Ok(SyncRagChunkRecord {
                chunk_id,
                session_id: row.try_get(1)?,
                source_kind: row.try_get(2)?,
                source_id: row.try_get(3)?,
                chunk_index: i64::from(chunk_index),
                text: row.try_get(5)?,
                // Embeddings are provider-derived server material. The desktop
                // reconstructs its own local index from the user-owned text.
                embedding: None,
                embedding_model: None,
                token_count,
                content_hash: row.try_get(7)?,
                updated_at_ms: row.try_get(8)?,
                deleted_at_ms: None,
                metadata: parse_json(&metadata),
            })
        })
        .collect()
}

fn load_child_tombstones_pg(
    conn: &mut Client,
    account_id: &str,
    session_id: &str,
) -> Result<Vec<CloudChildTombstone>> {
    let rows = conn.query(
        "SELECT child_kind, child_id, session_id, deleted_at_ms,
                source_kind, source_id, chunk_index
         FROM cloud_child_tombstones
         WHERE account_id = $1 AND session_id = $2
         ORDER BY deleted_at_ms ASC, child_kind ASC, child_id ASC",
        &[&account_id, &session_id],
    )?;
    let records = rows
        .into_iter()
        .map(|row| {
            Ok(CloudChildTombstone {
                child_kind: row.try_get(0)?,
                child_id: row.try_get(1)?,
                session_id: row.try_get(2)?,
                deleted_at_ms: row.try_get(3)?,
                source_kind: row.try_get(4)?,
                source_id: row.try_get(5)?,
                chunk_index: row.try_get(6)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    validate_loaded_child_tombstones(records, session_id)
}

fn validate_loaded_child_tombstones(
    records: Vec<CloudChildTombstone>,
    session_id: &str,
) -> Result<Vec<CloudChildTombstone>> {
    for record in &records {
        if record.session_id != session_id {
            anyhow::bail!("cloud child tombstone parent mismatch");
        }
        if !matches!(
            record.child_kind.as_str(),
            "transcript" | "response" | "context" | "rag"
        ) {
            anyhow::bail!("unsupported cloud child tombstone kind");
        }
        let provenance_count = [
            record.source_kind.is_some(),
            record.source_id.is_some(),
            record.chunk_index.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if record.child_kind == "rag" {
            if provenance_count != 0 && provenance_count != 3 {
                anyhow::bail!("RAG child tombstone has incomplete source provenance");
            }
            if record
                .source_kind
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
                || record
                    .source_id
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
            {
                anyhow::bail!("RAG child tombstone has empty source provenance");
            }
        } else if provenance_count != 0 {
            anyhow::bail!("non-RAG child tombstone has source provenance");
        }
    }
    Ok(records)
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

    fn insert_test_account(pool: &DbPool, account_id: &str, email: &str) {
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_id, email, "hash"],
            )
            .unwrap();
    }

    fn test_session(session_id: &str, updated_at_ms: i64) -> SyncSessionRecord {
        SyncSessionRecord {
            session_id: session_id.into(),
            title: "Test session".into(),
            status: "active".into(),
            created_at_ms: 1,
            updated_at_ms,
            last_active_at_ms: Some(updated_at_ms),
            answer_style: None,
            metadata: serde_json::json!({}),
            deleted_at_ms: None,
        }
    }

    fn test_response(session_id: &str, updated_at_ms: i64) -> SyncCueResponseRecord {
        SyncCueResponseRecord {
            response_id: format!("response-{session_id}"),
            session_id: session_id.to_string(),
            kind: "answer".to_string(),
            text: "answer".to_string(),
            source_text: None,
            ts_ms: updated_at_ms,
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
            metadata: serde_json::json!({}),
        }
    }

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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
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
        assert_eq!(bundle.rag_chunks.len(), 1);
        assert_eq!(bundle.rag_chunks[0].chunk_id, "c1");
        assert!(bundle.rag_chunks[0].embedding.is_none());
        assert!(bundle.rag_chunks[0].embedding_model.is_none());
        assert!(bundle.child_tombstones.is_empty());

        let matches = query_rag(&pool, account_id, "cache", Some(&[1.0, 0.0]), 5).unwrap();
        assert_eq!(matches[0].chunk_id, "c1");
        assert!(matches[0].score > 0.8);
    }

    #[test]
    fn live_session_existence_is_account_scoped_and_excludes_tombstones() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        insert_test_account(&pool, "acct_live_owner", "live-owner@example.test");
        insert_test_account(&pool, "acct_live_other", "live-other@example.test");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    metadata_json
                 ) VALUES (?1, ?2, 'Live', 'active', 1, 1, '{}')",
                rusqlite::params!["acct_live_owner", "live-session"],
            )
            .unwrap();

        assert!(live_session_exists(&pool, "acct_live_owner", "live-session").unwrap());
        assert!(!live_session_exists(&pool, "acct_live_other", "live-session").unwrap());
        assert!(!live_session_exists(&pool, "acct_live_owner", "missing-session").unwrap());

        pool.get()
            .unwrap()
            .execute(
                "UPDATE cloud_sessions SET deleted_at_ms = 2
                  WHERE account_id = ?1 AND session_id = ?2",
                rusqlite::params!["acct_live_owner", "live-session"],
            )
            .unwrap();
        assert!(!live_session_exists(&pool, "acct_live_owner", "live-session").unwrap());
    }

    #[test]
    fn session_bundle_returns_account_scoped_rag_and_child_tombstones() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_bundle_a";
        let other_account_id = "acct_bundle_b";
        insert_test_account(&pool, account_id, "bundle-a@example.com");
        insert_test_account(&pool, other_account_id, "bundle-b@example.com");

        let session = test_session("bundle-session-a", 10);
        let other_session = test_session("bundle-session-b", 10);
        let rag = |chunk_id: &str, session_id: &str| SyncRagChunkRecord {
            chunk_id: chunk_id.into(),
            session_id: Some(session_id.into()),
            source_kind: "memory".into(),
            source_id: format!("memory-{session_id}"),
            chunk_index: 0,
            text: format!("private memory for {session_id}"),
            embedding: None,
            embedding_model: None,
            token_count: Some(4),
            content_hash: None,
            updated_at_ms: 10,
            deleted_at_ms: None,
            metadata: serde_json::json!({"scope":"session"}),
        };
        upsert_batch(
            &pool,
            account_id,
            std::slice::from_ref(&session),
            &[],
            &[],
            &[],
            &[rag("rag-a", &session.session_id)],
        )
        .unwrap();
        upsert_batch(
            &pool,
            other_account_id,
            std::slice::from_ref(&other_session),
            &[],
            &[],
            &[],
            &[rag("rag-b", &other_session.session_id)],
        )
        .unwrap();

        let conn = pool.get().unwrap();
        for (child_kind, child_id) in [
            ("transcript", "transcript-a"),
            ("response", "response-a"),
            ("context", "context-a"),
            ("rag", "deleted-rag-a"),
        ] {
            conn.execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, 20)",
                params![account_id, child_kind, child_id, session.session_id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO cloud_child_tombstones (
                account_id, child_kind, child_id, session_id, deleted_at_ms
             ) VALUES (?1, 'response', 'response-b', ?2, 30)",
            params![other_account_id, other_session.session_id],
        )
        .unwrap();
        drop(conn);

        let bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .expect("owned session bundle");
        assert_eq!(
            bundle
                .rag_chunks
                .iter()
                .map(|record| record.chunk_id.as_str())
                .collect::<Vec<_>>(),
            ["rag-a"]
        );
        assert_eq!(bundle.child_tombstones.len(), 4);
        assert!(bundle.child_tombstones.iter().all(|record| {
            record.session_id == session.session_id
                && record.child_id != "response-b"
                && record.deleted_at_ms == 20
        }));
        let legacy_rag_marker = bundle
            .child_tombstones
            .iter()
            .find(|record| record.child_kind == "rag")
            .expect("legacy RAG tombstone remains visible");
        assert!(legacy_rag_marker.source_kind.is_none());
        assert!(legacy_rag_marker.source_id.is_none());
        assert!(legacy_rag_marker.chunk_index.is_none());
    }

    #[test]
    fn retained_child_tombstone_cannot_be_rebound_to_another_session() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_tombstone_parent";
        insert_test_account(&pool, account_id, "tombstone-parent@example.com");
        let first = test_session("first-session", 10);
        let second = test_session("second-session", 10);
        upsert_batch(
            &pool,
            account_id,
            &[first.clone(), second.clone()],
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES (?1, 'context', 'stable-artifact-id', ?2, 20)",
                params![account_id, first.session_id],
            )
            .unwrap();

        let error = upsert_batch(
            &pool,
            account_id,
            &[],
            &[],
            &[],
            &[SyncContextArtifactRecord {
                artifact_id: "stable-artifact-id".into(),
                session_id: second.session_id,
                kind: "document".into(),
                title: "Must not be rebound".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: None,
                created_at_ms: 30,
                updated_at_ms: 30,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
            &[],
        )
        .expect_err("a retained deletion marker must keep its original parent");
        assert!(matches!(
            error.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::ParentMismatch { .. })
        ));
    }

    #[test]
    fn exact_retry_backfills_legacy_rag_tombstone_provenance() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_rag_tombstone_upgrade";
        let session = test_session("rag-tombstone-upgrade", 10);
        insert_test_account(&pool, account_id, "rag-upgrade@example.com");
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
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES (?1, 'rag', 'legacy-rag-id', ?2, 20)",
                params![account_id, session.session_id],
            )
            .unwrap();

        let counts = upsert_batch(
            &pool,
            account_id,
            &[],
            &[],
            &[],
            &[],
            &[SyncRagChunkRecord {
                chunk_id: "legacy-rag-id".into(),
                session_id: Some(session.session_id.clone()),
                source_kind: "conversation_memory".into(),
                source_id: "memory-epoch-3".into(),
                chunk_index: 3,
                text: String::new(),
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: 20,
                deleted_at_ms: Some(20),
                metadata: serde_json::json!({"tombstone":true}),
            }],
        )
        .unwrap();
        assert_eq!(counts.rag_chunks, 1);

        let bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .expect("session bundle");
        let marker = bundle
            .child_tombstones
            .iter()
            .find(|marker| marker.child_id == "legacy-rag-id")
            .expect("upgraded RAG marker");
        assert_eq!(marker.source_kind.as_deref(), Some("conversation_memory"));
        assert_eq!(marker.source_id.as_deref(), Some("memory-epoch-3"));
        assert_eq!(marker.chunk_index, Some(3));
    }

    #[test]
    fn session_pages_are_stable_complete_and_account_scoped() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_page_a";
        let other_account_id = "acct_page_b";
        insert_test_account(&pool, account_id, "page-a@example.com");
        insert_test_account(&pool, other_account_id, "page-b@example.com");

        let live_sessions = [
            test_session("live-z", 30),
            test_session("live-b", 20),
            test_session("live-a", 20),
            test_session("live-low", 10),
        ];
        let live_responses = live_sessions
            .iter()
            .map(|session| test_response(&session.session_id, session.updated_at_ms))
            .collect::<Vec<_>>();
        upsert_batch(
            &pool,
            account_id,
            &live_sessions,
            &[],
            &live_responses,
            &[],
            &[],
        )
        .unwrap();
        upsert_batch(
            &pool,
            other_account_id,
            &[test_session("other-private-live", 100)],
            &[],
            &[test_response("other-private-live", 100)],
            &[],
            &[],
        )
        .unwrap();

        let deleted_sessions = [
            test_session("deleted-z", 4),
            test_session("deleted-b", 3),
            test_session("deleted-a", 2),
            test_session("deleted-low", 1),
        ];
        upsert_batch(&pool, account_id, &deleted_sessions, &[], &[], &[], &[]).unwrap();
        upsert_batch(
            &pool,
            other_account_id,
            &[test_session("other-private-deleted", 100)],
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap();
        let conn = pool.get().unwrap();
        for (session_id, deleted_at_ms) in [
            ("deleted-z", 30),
            ("deleted-b", 20),
            ("deleted-a", 20),
            ("deleted-low", 10),
        ] {
            conn.execute(
                "UPDATE cloud_sessions SET deleted_at_ms = ?3
                 WHERE account_id = ?1 AND session_id = ?2",
                params![account_id, session_id, deleted_at_ms],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE cloud_sessions SET deleted_at_ms = 100
             WHERE account_id = ?1 AND session_id = 'other-private-deleted'",
            params![other_account_id],
        )
        .unwrap();
        drop(conn);

        let first = list_sessions_page(&pool, account_id, 2, None).unwrap();
        assert_eq!(
            first
                .sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            ["live-z", "live-b"]
        );
        let second = list_sessions_page(&pool, account_id, 2, first.next_cursor.as_ref()).unwrap();
        assert_eq!(
            second
                .sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            ["live-a", "live-low"]
        );
        assert!(second.next_cursor.is_none());

        let first = list_deleted_sessions_page(&pool, account_id, 2, None).unwrap();
        assert_eq!(
            first
                .sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            ["deleted-z", "deleted-b"]
        );
        let second =
            list_deleted_sessions_page(&pool, account_id, 2, first.next_cursor.as_ref()).unwrap();
        assert_eq!(
            second
                .sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            ["deleted-a", "deleted-low"]
        );
        assert!(second.next_cursor.is_none());
    }

    #[test]
    fn session_pages_traverse_more_than_the_api_page_limit() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_large_history";
        insert_test_account(&pool, account_id, "large-history@example.com");

        let live_sessions = (0..425)
            .map(|index| test_session(&format!("live-{index:04}"), 10_000 - index))
            .collect::<Vec<_>>();
        let live_responses = live_sessions
            .iter()
            .map(|session| test_response(&session.session_id, session.updated_at_ms))
            .collect::<Vec<_>>();
        upsert_batch(
            &pool,
            account_id,
            &live_sessions,
            &[],
            &live_responses,
            &[],
            &[],
        )
        .unwrap();

        let deleted_sessions = (0..425)
            .map(|index| test_session(&format!("deleted-{index:04}"), 20_000 - index))
            .collect::<Vec<_>>();
        upsert_batch(&pool, account_id, &deleted_sessions, &[], &[], &[], &[]).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE cloud_sessions
             SET deleted_at_ms = updated_at_ms
             WHERE account_id = ?1 AND session_id LIKE 'deleted-%'",
            params![account_id],
        )
        .unwrap();
        drop(conn);

        let mut live_ids = Vec::new();
        let mut live_cursor = None;
        loop {
            let page = list_sessions_page(&pool, account_id, 200, live_cursor.as_ref()).unwrap();
            live_ids.extend(page.sessions.into_iter().map(|session| session.session_id));
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            live_cursor = Some(next_cursor);
        }

        let mut deleted_ids = Vec::new();
        let mut deleted_cursor = None;
        loop {
            let page = list_deleted_sessions_page(&pool, account_id, 200, deleted_cursor.as_ref())
                .unwrap();
            deleted_ids.extend(page.sessions.into_iter().map(|session| session.session_id));
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            deleted_cursor = Some(next_cursor);
        }

        assert_eq!(live_ids.len(), 425);
        assert_eq!(deleted_ids.len(), 425);
        assert_eq!(live_ids.iter().collect::<HashSet<_>>().len(), 425);
        assert_eq!(deleted_ids.iter().collect::<HashSet<_>>().len(), 425);
    }

    #[test]
    fn child_tombstones_remove_all_child_records_and_enforce_newer_writes() {
        use crate::db::object_uploads::{
            mark_upload_ready, reserve_upload, NewObjectUpload, ObjectKind, StorageScope,
        };
        use crate::object_storage::{sha256_hex, UploadLimits};

        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_child_delete";
        insert_test_account(&pool, account_id, "child-delete@example.com");
        let session = test_session("session-child-delete", 10);
        let transcript = SyncTranscriptSegment {
            segment_id: "segment-private".into(),
            session_id: session.session_id.clone(),
            speaker: "user".into(),
            source: "microphone".into(),
            text: "private transcript text".into(),
            start_ms: Some(0),
            end_ms: Some(1000),
            ts_ms: 10,
            is_final: true,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };
        let response = SyncCueResponseRecord {
            response_id: "response-private".into(),
            session_id: session.session_id.clone(),
            kind: "answer".into(),
            text: "private response text".into(),
            source_text: Some("private question".into()),
            ts_ms: 10,
            provider: Some("test".into()),
            model: Some("test".into()),
            lane: None,
            task_type: None,
            cost_cents: None,
            balance_cents_after: None,
            cost_label: None,
            artifact_type: None,
            artifact_body: None,
            artifact_confidence: None,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };
        let context = SyncContextArtifactRecord {
            artifact_id: "artifact-private".into(),
            session_id: session.session_id.clone(),
            kind: "document".into(),
            title: "Private context".into(),
            note: None,
            source_uri: Some("bluey://artifact/artifact-private".into()),
            content_hash: None,
            text_preview: Some("private context text".into()),
            created_at_ms: 10,
            updated_at_ms: 10,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
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
        let object_hash = sha256_hex("private context bytes");
        let object_reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: context.artifact_id.clone(),
                session_id: Some(session.session_id.clone()),
                storage_scope: StorageScope::Artifact,
                object_key: format!(
                    "objects/accounts/{account_id}/{}/{object_hash}",
                    context.artifact_id
                ),
                size_bytes: 21,
                sha256: object_hash,
                content_type: "text/plain".into(),
                expires_at_ms: 86_400_010,
                metadata_json: serde_json::json!({}),
                now_ms: 10,
                limits: UploadLimits {
                    max_object_bytes: 100,
                    max_account_bytes: 1_000,
                    max_daily_bytes: 1_000,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        mark_upload_ready(&pool, &object_reservation.upload.id, 11).unwrap();
        let orphan_artifact_id = "artifact-uploaded-before-metadata";
        let orphan_hash = sha256_hex("uploaded before metadata");
        let orphan_reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: orphan_artifact_id.into(),
                session_id: Some(session.session_id.clone()),
                storage_scope: StorageScope::Artifact,
                object_key: format!(
                    "objects/accounts/{account_id}/{orphan_artifact_id}/{orphan_hash}"
                ),
                size_bytes: 24,
                sha256: orphan_hash,
                content_type: "text/plain".into(),
                expires_at_ms: 86_400_010,
                metadata_json: serde_json::json!({}),
                now_ms: 10,
                limits: UploadLimits {
                    max_object_bytes: 100,
                    max_account_bytes: 1_000,
                    max_daily_bytes: 1_000,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        mark_upload_ready(&pool, &orphan_reservation.upload.id, 11).unwrap();
        let rag = SyncRagChunkRecord {
            chunk_id: "session-child-delete:context:artifact-private:0".into(),
            session_id: Some(session.session_id.clone()),
            source_kind: "context".into(),
            source_id: context.artifact_id.clone(),
            chunk_index: 0,
            text: "private context text".into(),
            embedding: Some(vec![1.0, 0.0]),
            embedding_model: Some("test".into()),
            token_count: None,
            content_hash: None,
            updated_at_ms: 10,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };
        upsert_batch(
            &pool,
            account_id,
            std::slice::from_ref(&session),
            std::slice::from_ref(&transcript),
            std::slice::from_ref(&response),
            std::slice::from_ref(&context),
            std::slice::from_ref(&rag),
        )
        .unwrap();
        let initial_bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(initial_bundle.transcript_segments.len(), 1);
        assert_eq!(initial_bundle.cue_responses.len(), 1);
        assert_eq!(initial_bundle.context_artifacts.len(), 1);
        assert_eq!(initial_bundle.rag_chunks.len(), 1);
        assert!(initial_bundle.child_tombstones.is_empty());
        assert_eq!(
            query_rag(&pool, account_id, "private", Some(&[1.0, 0.0]), 5)
                .unwrap()
                .len(),
            1
        );

        let mut transcript_tombstone = transcript.clone();
        transcript_tombstone.text.clear();
        transcript_tombstone.ts_ms = 20;
        transcript_tombstone.deleted_at_ms = Some(20);
        let mut response_tombstone = response.clone();
        response_tombstone.text.clear();
        response_tombstone.source_text = None;
        response_tombstone.ts_ms = 20;
        response_tombstone.deleted_at_ms = Some(20);
        let mut context_tombstone = context.clone();
        context_tombstone.text_preview = None;
        context_tombstone.updated_at_ms = 20;
        context_tombstone.deleted_at_ms = Some(20);
        let orphan_context_tombstone = SyncContextArtifactRecord {
            artifact_id: orphan_artifact_id.into(),
            session_id: session.session_id.clone(),
            kind: "document".into(),
            title: "Upload without metadata".into(),
            note: None,
            source_uri: None,
            content_hash: None,
            text_preview: None,
            created_at_ms: 10,
            updated_at_ms: 20,
            deleted_at_ms: Some(20),
            metadata: serde_json::json!({}),
        };
        let mut rag_tombstone = rag.clone();
        rag_tombstone.text.clear();
        rag_tombstone.embedding = None;
        rag_tombstone.updated_at_ms = 20;
        rag_tombstone.deleted_at_ms = Some(20);
        upsert_batch(
            &pool,
            account_id,
            &[],
            &[transcript_tombstone],
            &[response_tombstone],
            &[context_tombstone, orphan_context_tombstone],
            &[rag_tombstone],
        )
        .unwrap();

        let bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .unwrap();
        assert!(bundle.transcript_segments.is_empty());
        assert!(bundle.cue_responses.is_empty());
        assert!(bundle.context_artifacts.is_empty());
        assert!(bundle.rag_chunks.is_empty());
        assert_eq!(bundle.child_tombstones.len(), 5);
        let listed = list_sessions(&pool, account_id, 10).unwrap();
        assert_eq!(
            listed.len(),
            1,
            "tombstone-only sessions must remain discoverable"
        );
        assert_eq!(listed[0].updated_at_ms, 20);
        assert_eq!(listed[0].transcript_count, 0);
        assert_eq!(listed[0].response_count, 0);
        assert_eq!(listed[0].context_count, 0);
        assert_eq!(listed[0].rag_count, 0);
        assert_eq!(listed[0].child_tombstone_count, 5);
        assert_eq!(listed[0].child_tombstone_updated_at_ms, Some(20));
        assert_eq!(
            bundle
                .child_tombstones
                .iter()
                .map(|record| record.child_kind.as_str())
                .collect::<Vec<_>>(),
            ["context", "context", "rag", "response", "transcript"]
        );
        assert!(bundle
            .child_tombstones
            .iter()
            .all(|record| record.deleted_at_ms == 20));
        let tombstone_json = serde_json::to_value(&bundle.child_tombstones).unwrap();
        for marker in tombstone_json.as_array().unwrap() {
            let marker = marker.as_object().unwrap();
            if marker["child_kind"] == "rag" {
                assert_eq!(marker.len(), 7, "RAG tombstones carry identifiers only");
                assert_eq!(marker["source_kind"], "context");
                assert_eq!(marker["source_id"], "artifact-private");
                assert_eq!(marker["chunk_index"], 0);
            } else {
                assert_eq!(marker.len(), 4, "non-RAG tombstones carry no payload");
            }
            assert!(!marker.contains_key("text"));
            assert!(!marker.contains_key("title"));
            assert!(!marker.contains_key("metadata"));
            assert!(!marker.contains_key("source_uri"));
        }
        assert!(
            query_rag(&pool, account_id, "private", Some(&[1.0, 0.0]), 5)
                .unwrap()
                .is_empty()
        );
        let object_lifecycle: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                params![object_reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            object_lifecycle,
            ("delete_pending".into(), "pending".into())
        );
        let orphan_object_lifecycle: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                params![orphan_reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            orphan_object_lifecycle,
            ("delete_pending".into(), "pending".into())
        );

        // A delayed upload from another device cannot resurrect content that
        // predates the tombstone.
        upsert_batch(
            &pool,
            account_id,
            &[],
            std::slice::from_ref(&transcript),
            std::slice::from_ref(&response),
            std::slice::from_ref(&context),
            std::slice::from_ref(&rag),
        )
        .unwrap();
        let stale_bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .unwrap();
        assert!(stale_bundle.transcript_segments.is_empty());
        assert!(stale_bundle.cue_responses.is_empty());
        assert!(stale_bundle.context_artifacts.is_empty());
        assert!(stale_bundle.rag_chunks.is_empty());
        assert_eq!(stale_bundle.child_tombstones.len(), 5);
        assert!(
            query_rag(&pool, account_id, "private", Some(&[1.0, 0.0]), 5)
                .unwrap()
                .is_empty()
        );

        // Ordinary text records retain last-write-wins, but a deleted context
        // artifact id is final. Reusing it could publish metadata or RAG text
        // after its durable object has already entered deletion.
        let mut newer_transcript = transcript;
        newer_transcript.text = "newer transcript text".into();
        newer_transcript.ts_ms = 30;
        let mut newer_response = response;
        newer_response.text = "newer response text".into();
        newer_response.ts_ms = 30;
        let mut newer_context = context;
        newer_context.text_preview = Some("newer context text".into());
        newer_context.updated_at_ms = 30;
        let mut newer_rag = rag;
        newer_rag.text = "newer context text".into();
        newer_rag.updated_at_ms = 30;
        upsert_batch(
            &pool,
            account_id,
            &[],
            &[newer_transcript],
            &[newer_response],
            &[newer_context],
            &[newer_rag],
        )
        .unwrap();
        let newer_bundle = load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            newer_bundle.transcript_segments[0].text,
            "newer transcript text"
        );
        assert_eq!(newer_bundle.cue_responses[0].text, "newer response text");
        assert!(newer_bundle.context_artifacts.is_empty());
        assert!(newer_bundle.rag_chunks.is_empty());
        assert_eq!(
            newer_bundle
                .child_tombstones
                .iter()
                .map(|record| record.child_kind.as_str())
                .collect::<Vec<_>>(),
            ["context", "context", "rag"]
        );
        assert!(
            query_rag(&pool, account_id, "newer", Some(&[1.0, 0.0]), 5)
                .unwrap()
                .is_empty(),
            "RAG derived from a finally deleted context id must not resurrect"
        );
    }

    #[test]
    fn delete_pending_object_prevents_metadata_only_context_resurrection() {
        use crate::db::object_uploads::{
            mark_upload_ready, reserve_upload, schedule_artifact_cleanup_sqlite_tx,
            NewObjectUpload, ObjectKind, StorageScope,
        };
        use crate::object_storage::{sha256_hex, UploadLimits};

        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_final_context_delete";
        let session = test_session("session-final-context-delete", 10);
        let artifact_id = "artifact-final-context-delete";
        insert_test_account(&pool, account_id, "final-context@example.test");
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

        let hash = sha256_hex("private bytes");
        let reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: artifact_id.into(),
                session_id: Some(session.session_id.clone()),
                storage_scope: StorageScope::Artifact,
                object_key: format!("objects/accounts/{account_id}/{artifact_id}/{hash}"),
                size_bytes: 13,
                sha256: hash.clone(),
                content_type: "text/plain".into(),
                expires_at_ms: 86_400_010,
                metadata_json: serde_json::json!({}),
                now_ms: 10,
                limits: UploadLimits {
                    max_object_bytes: 100,
                    max_account_bytes: 1_000,
                    max_daily_bytes: 1_000,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        mark_upload_ready(&pool, &reservation.upload.id, 11).unwrap();
        {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            schedule_artifact_cleanup_sqlite_tx(&tx, account_id, artifact_id, 20).unwrap();
            tx.commit().unwrap();
        }

        let context = SyncContextArtifactRecord {
            artifact_id: artifact_id.into(),
            session_id: session.session_id.clone(),
            kind: "document".into(),
            title: "Metadata without durable bytes".into(),
            note: None,
            source_uri: None,
            content_hash: Some(hash),
            text_preview: Some("must not return".into()),
            created_at_ms: 10,
            updated_at_ms: 30,
            deleted_at_ms: None,
            metadata: serde_json::json!({
                "object_key": reservation.upload.object_key,
                "object_size_bytes": reservation.upload.size_bytes,
            }),
        };
        let rag = SyncRagChunkRecord {
            chunk_id: "rag-final-context-delete".into(),
            session_id: Some(session.session_id.clone()),
            source_kind: "context".into(),
            source_id: artifact_id.into(),
            chunk_index: 0,
            text: "must not return".into(),
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms: 30,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };
        let counts = upsert_batch(&pool, account_id, &[], &[], &[], &[context], &[rag]).unwrap();
        assert_eq!(counts.context_artifacts, 0);
        assert_eq!(counts.rag_chunks, 0);
        assert!(load_session(&pool, account_id, &session.session_id)
            .unwrap()
            .unwrap()
            .context_artifacts
            .is_empty());
        assert!(query_rag(&pool, account_id, "must not return", None, 5)
            .unwrap()
            .is_empty());
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
                deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
            deleted_at_ms: None,
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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
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
                deleted_at_ms: None,
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
    fn batch_session_tombstone_is_rejected_before_partial_deletion() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let account_id = "acct_batch_delete_rejected";
        insert_test_account(&pool, account_id, "batch-delete@example.test");
        let mut session = test_session("session-batch-delete", 20);
        session.status = "deleted".into();
        session.deleted_at_ms = Some(20);

        let error = upsert_batch(&pool, account_id, &[session], &[], &[], &[], &[])
            .expect_err("batch deletion must use the atomic DELETE path");
        assert!(matches!(
            error.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::UnsupportedSessionTombstone)
        ));
        assert!(list_sessions(&pool, account_id, 10).unwrap().is_empty());
        assert!(list_deleted_sessions(&pool, account_id, 10)
            .unwrap()
            .is_empty());
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
            &[],
            &[],
        )
        .unwrap();
        let hash = sha256_hex("artifact bytes");
        let reservation = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: artifact_id.clone(),
                session_id: Some(session_id.clone()),
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

        // This second upload intentionally never receives context metadata,
        // modeling a crash between durable object reservation and batch sync.
        let orphan_artifact_id = uuid::Uuid::new_v4().to_string();
        let orphan_hash = sha256_hex("orphaned artifact bytes");
        let orphan = reserve_upload(
            &pool,
            &NewObjectUpload {
                account_id: account_id.into(),
                object_kind: ObjectKind::Artifact,
                logical_id: orphan_artifact_id.clone(),
                session_id: Some(session_id.clone()),
                storage_scope: StorageScope::Artifact,
                object_key: format!(
                    "objects/accounts/{account_id}/{orphan_artifact_id}/{orphan_hash}"
                ),
                size_bytes: 23,
                sha256: orphan_hash,
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
        assert_eq!(
            orphan.upload.session_id.as_deref(),
            Some(session_id.as_str())
        );
        mark_upload_ready(&pool, &orphan.upload.id, created_at_ms + 1).unwrap();

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
                deleted_at_ms: None,
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
        let orphan_lifecycle: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                rusqlite::params![orphan.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            orphan_lifecycle,
            ("delete_pending".into(), "pending".into()),
            "the parent-bound reservation remains deletable without metadata"
        );
    }

    #[test]
    fn sync_rejects_cross_account_missing_parent_and_child_reparenting() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        insert_test_account(&pool, "acct_owner_a", "owner-a@example.com");
        insert_test_account(&pool, "acct_owner_b", "owner-b@example.com");

        upsert_batch(
            &pool,
            "acct_owner_a",
            &[
                test_session("session-a", 10),
                test_session("session-a-2", 10),
            ],
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap();
        upsert_batch(
            &pool,
            "acct_owner_b",
            &[test_session("session-b", 10)],
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap();

        let cross_account_session = upsert_batch(
            &pool,
            "acct_owner_b",
            &[test_session("session-a", 11)],
            &[],
            &[],
            &[],
            &[],
        )
        .unwrap_err();
        assert!(matches!(
            cross_account_session.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::CrossAccountIdentity {
                entity: "session",
                ..
            })
        ));

        let segment = SyncTranscriptSegment {
            segment_id: "stable-segment".into(),
            session_id: "session-a".into(),
            speaker: "user".into(),
            source: "microphone".into(),
            text: "owned transcript".into(),
            start_ms: Some(1),
            end_ms: Some(2),
            ts_ms: 12,
            is_final: true,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };
        upsert_batch(
            &pool,
            "acct_owner_a",
            &[],
            std::slice::from_ref(&segment),
            &[],
            &[],
            &[],
        )
        .unwrap();

        let mut moved = segment.clone();
        moved.session_id = "session-a-2".into();
        let reparented =
            upsert_batch(&pool, "acct_owner_a", &[], &[moved], &[], &[], &[]).unwrap_err();
        assert!(matches!(
            reparented.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::ParentMismatch {
                entity: "transcript segment",
                ..
            })
        ));

        let mut reused = segment.clone();
        reused.session_id = "session-b".into();
        let cross_account_child =
            upsert_batch(&pool, "acct_owner_b", &[], &[reused], &[], &[], &[]).unwrap_err();
        assert!(matches!(
            cross_account_child.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::CrossAccountIdentity {
                entity: "transcript segment",
                ..
            })
        ));

        let mut dangling = segment;
        dangling.segment_id = "dangling-segment".into();
        dangling.session_id = "missing-session".into();
        let missing_parent =
            upsert_batch(&pool, "acct_owner_a", &[], &[dangling], &[], &[], &[]).unwrap_err();
        assert!(matches!(
            missing_parent.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::MissingParent { .. })
        ));

        upsert_batch(
            &pool,
            "acct_owner_a",
            &[],
            &[],
            &[],
            &[SyncContextArtifactRecord {
                artifact_id: "owned-attachment".into(),
                session_id: "session-a".into(),
                kind: "document".into(),
                title: "Owned".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: Some("owned".into()),
                created_at_ms: 10,
                updated_at_ms: 10,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
            &[],
        )
        .unwrap();
        let wrong_parent_attachment = upsert_batch(
            &pool,
            "acct_owner_a",
            &[],
            &[],
            &[SyncCueResponseRecord {
                response_id: "response-with-wrong-attachment".into(),
                session_id: "session-a-2".into(),
                kind: "answer".into(),
                text: "answer".into(),
                source_text: Some("question".into()),
                ts_ms: 20,
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
                metadata: serde_json::json!({"attachment_ids": ["owned-attachment"]}),
            }],
            &[],
            &[],
        )
        .unwrap_err();
        assert!(matches!(
            wrong_parent_attachment.downcast_ref::<SyncWriteError>(),
            Some(SyncWriteError::ParentMismatch {
                entity: "attachment",
                ..
            })
        ));
    }

    #[test]
    fn exact_retry_keeps_one_owned_row_and_preserves_stt_metadata() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        insert_test_account(&pool, "acct_retry", "retry@example.com");

        let session = test_session("retry-session", 10);
        let transcript = SyncTranscriptSegment {
            segment_id: "retry-segment".into(),
            session_id: "retry-session".into(),
            speaker: "other".into(),
            source: "system_audio".into(),
            text: "retry transcript".into(),
            start_ms: Some(101),
            end_ms: Some(202),
            ts_ms: 20,
            is_final: true,
            deleted_at_ms: None,
            metadata: serde_json::json!({"stt_provider": "deepgram", "channel": 2}),
        };
        let context = SyncContextArtifactRecord {
            artifact_id: "retry-artifact".into(),
            session_id: "retry-session".into(),
            kind: "document".into(),
            title: "Retry document".into(),
            note: None,
            source_uri: Some("/original/retry.pdf".into()),
            content_hash: Some("abc123".into()),
            text_preview: Some("retry context".into()),
            created_at_ms: 30,
            updated_at_ms: 30,
            deleted_at_ms: None,
            metadata: serde_json::json!({"processing_status": "ready"}),
        };
        let response = SyncCueResponseRecord {
            response_id: "retry-response".into(),
            session_id: "retry-session".into(),
            kind: "answer".into(),
            text: "retry answer".into(),
            source_text: Some("retry question".into()),
            ts_ms: 40,
            provider: Some("bluey_managed".into()),
            model: Some("balanced".into()),
            lane: Some("balanced".into()),
            task_type: Some("general".into()),
            cost_cents: None,
            balance_cents_after: None,
            cost_label: None,
            artifact_type: Some("code".into()),
            artifact_body: Some("fn retry() {}".into()),
            artifact_confidence: Some(0.9),
            deleted_at_ms: None,
            metadata: serde_json::json!({
                "attachment_ids": ["retry-artifact"],
                "canvas_artifact_id": "retry-canvas"
            }),
        };
        let rag = SyncRagChunkRecord {
            chunk_id: "retry-session:response:retry-response:0".into(),
            session_id: Some("retry-session".into()),
            source_kind: "response".into(),
            source_id: "retry-response".into(),
            chunk_index: 0,
            text: "retry answer".into(),
            embedding: None,
            embedding_model: None,
            token_count: None,
            content_hash: None,
            updated_at_ms: 40,
            deleted_at_ms: None,
            metadata: serde_json::json!({}),
        };

        for _ in 0..2 {
            upsert_batch(
                &pool,
                "acct_retry",
                std::slice::from_ref(&session),
                std::slice::from_ref(&transcript),
                std::slice::from_ref(&response),
                std::slice::from_ref(&context),
                std::slice::from_ref(&rag),
            )
            .unwrap();
        }

        let conn = pool.get().unwrap();
        for table in [
            "cloud_sessions",
            "cloud_transcript_segments",
            "cloud_cue_responses",
            "cloud_context_artifacts",
            "cloud_rag_chunks",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE account_id = ?1"),
                    rusqlite::params!["acct_retry"],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "retry duplicated {table}");
        }
        drop(conn);

        let bundle = load_session(&pool, "acct_retry", "retry-session")
            .unwrap()
            .unwrap();
        let restored = &bundle.transcript_segments[0];
        assert_eq!(restored.source, "system_audio");
        assert_eq!(restored.start_ms, Some(101));
        assert_eq!(restored.end_ms, Some(202));
        assert_eq!(restored.ts_ms, 20);
        assert_eq!(restored.metadata["stt_provider"], "deepgram");
        assert_eq!(bundle.cue_responses[0].session_id, "retry-session");
        assert_eq!(bundle.context_artifacts[0].session_id, "retry-session");
    }

    #[test]
    fn listed_session_revision_includes_transcript_response_context_and_rag() {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        insert_test_account(&pool, "acct_revision", "revision@example.com");

        upsert_batch(
            &pool,
            "acct_revision",
            &[test_session("revision-session", 10)],
            &[SyncTranscriptSegment {
                segment_id: "revision-segment".into(),
                session_id: "revision-session".into(),
                speaker: "user".into(),
                source: "unknown".into(),
                text: "transcript".into(),
                start_ms: None,
                end_ms: None,
                ts_ms: 20,
                is_final: true,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
            &[SyncCueResponseRecord {
                response_id: "revision-response".into(),
                session_id: "revision-session".into(),
                kind: "answer".into(),
                text: "answer".into(),
                source_text: Some("question".into()),
                ts_ms: 30,
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
                metadata: serde_json::json!({}),
            }],
            &[SyncContextArtifactRecord {
                artifact_id: "revision-artifact".into(),
                session_id: "revision-session".into(),
                kind: "document".into(),
                title: "revision context".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: Some("context".into()),
                created_at_ms: 1,
                updated_at_ms: 40,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
            &[SyncRagChunkRecord {
                chunk_id: "revision-rag".into(),
                session_id: Some("revision-session".into()),
                source_kind: "context".into(),
                source_id: "revision-artifact".into(),
                chunk_index: 0,
                text: "rag".into(),
                embedding: None,
                embedding_model: None,
                token_count: None,
                content_hash: None,
                updated_at_ms: 35,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
        )
        .unwrap();
        assert_eq!(
            list_sessions(&pool, "acct_revision", 10).unwrap()[0].updated_at_ms,
            40
        );

        upsert_batch(
            &pool,
            "acct_revision",
            &[],
            &[],
            &[],
            &[SyncContextArtifactRecord {
                artifact_id: "revision-artifact".into(),
                session_id: "revision-session".into(),
                kind: "document".into(),
                title: "newer context".into(),
                note: None,
                source_uri: None,
                content_hash: None,
                text_preview: Some("newer context".into()),
                created_at_ms: 1,
                updated_at_ms: 50,
                deleted_at_ms: None,
                metadata: serde_json::json!({}),
            }],
            &[],
        )
        .unwrap();
        assert_eq!(
            list_sessions(&pool, "acct_revision", 10).unwrap()[0].updated_at_ms,
            50
        );
    }
}
