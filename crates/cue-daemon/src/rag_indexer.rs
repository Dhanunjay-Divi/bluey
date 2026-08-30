use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use cue_cloud_client::TokenStore;
use cue_core::{
    app_paths::AppPaths, load_account, load_settings, new_request_id, ContextArtifact,
    MeetingRecord,
};
use cue_rag::{EmbeddingError, EmbeddingProvider, RagScope};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, info, warn};

use crate::db::rag_queue::RagIndexQueue;
use crate::storage::MeetingStore;

const MANAGED_EMBED_DIM: usize = cue_rag::embedder::OpenAiEmbedder::DIM;
const MAX_MANAGED_EMBED_INPUT_CHARS: usize = 8_192;
const LOCAL_RAG_ACCOUNT_ID: &str = "__bluey_local_account__";
const RAG_INDEX_QUEUE_FILE: &str = "rag-index-jobs.db";
const RAG_INDEX_LEASE_TTL: Duration = Duration::from_secs(15 * 60);
const RAG_INDEX_IDLE_POLL: Duration = Duration::from_secs(30);

struct ManagedBlueyEmbedder {
    client: cue_cloud_client::CloudClient,
    paths: AppPaths,
}

impl ManagedBlueyEmbedder {
    fn new(client: cue_cloud_client::CloudClient, paths: AppPaths) -> Self {
        Self { client, paths }
    }
}

#[async_trait]
impl EmbeddingProvider for ManagedBlueyEmbedder {
    fn name(&self) -> &'static str {
        "bluey-managed"
    }

    fn model(&self) -> &'static str {
        cue_rag::embedder::OpenAiEmbedder::MODEL
    }

    fn dim(&self) -> usize {
        MANAGED_EMBED_DIM
    }

    async fn embed(&self, text: &str) -> std::result::Result<Vec<f32>, EmbeddingError> {
        let mut vectors = self.embed_batch(&[text.to_string()]).await?;
        vectors
            .pop()
            .ok_or_else(|| EmbeddingError::InvalidResponse("missing embedding".to_string()))
    }

    async fn embed_batch(
        &self,
        texts: &[String],
    ) -> std::result::Result<Vec<Vec<f32>>, EmbeddingError> {
        ensure_managed_embedding_consent(&self.paths)?;
        let inputs = texts
            .iter()
            .map(|text| bounded_embed_input(text))
            .filter(|input| !input.trim().is_empty())
            .collect::<Vec<_>>();
        if inputs.is_empty() {
            return Err(EmbeddingError::InvalidResponse(
                "empty embedding input".to_string(),
            ));
        }

        // Re-read persisted consent at the network boundary. This prevents a
        // coordinator/provider retained across a settings change from using
        // an earlier consent snapshot.
        ensure_managed_embedding_consent(&self.paths)?;
        let response = match self
            .client
            .embed_batch(&cue_cloud_client::EmbedBatchRequest {
                request_id: new_request_id(),
                inputs: inputs.clone(),
                model: Some(cue_rag::embedder::OpenAiEmbedder::MODEL.to_string()),
            })
            .await
        {
            Ok(response) => response,
            Err(cue_cloud_client::Error::Server { status }) if status == 404 || status == 405 => {
                let mut vectors = Vec::with_capacity(inputs.len());
                for input in inputs {
                    ensure_managed_embedding_consent(&self.paths)?;
                    let response = self
                        .client
                        .embed(&cue_cloud_client::EmbedRequest {
                            request_id: new_request_id(),
                            input,
                            model: Some(cue_rag::embedder::OpenAiEmbedder::MODEL.to_string()),
                        })
                        .await
                        .map_err(map_cloud_embed_error)?;
                    vectors.push(response.vector);
                }
                return validate_managed_vectors(vectors, texts.len());
            }
            Err(error) => return Err(map_cloud_embed_error(error)),
        };

        validate_managed_vectors(response.vectors, texts.len())
    }
}

fn validate_managed_vectors(
    vectors: Vec<Vec<f32>>,
    expected_count: usize,
) -> std::result::Result<Vec<Vec<f32>>, EmbeddingError> {
    if vectors.len() != expected_count {
        return Err(EmbeddingError::InvalidResponse(format!(
            "managed embedding returned {} vectors, expected {}",
            vectors.len(),
            expected_count
        )));
    }
    if let Some(bad) = vectors
        .iter()
        .find(|vector| vector.len() != MANAGED_EMBED_DIM)
    {
        return Err(EmbeddingError::InvalidResponse(format!(
            "managed embedding returned {} dimensions, expected {MANAGED_EMBED_DIM}",
            bad.len()
        )));
    }
    Ok(vectors)
}

#[derive(Clone)]
pub(crate) struct RagIndexCoordinator {
    pipeline: Arc<RwLock<Option<Arc<crate::db::rag::RagPipeline>>>>,
    session_lock: Arc<Mutex<()>>,
    queue: RagIndexQueue,
    store: MeetingStore,
    worker_id: Arc<str>,
    worker_started: Arc<AtomicBool>,
    worker_notify: Arc<Notify>,
    scope_epoch: Arc<AtomicU64>,
    scope_changed: Arc<Notify>,
    paths: AppPaths,
}

impl RagIndexCoordinator {
    pub(crate) fn from_paths(paths: &AppPaths, store: MeetingStore) -> Result<Self> {
        let coordinator = Self {
            pipeline: Arc::new(RwLock::new(init_rag_pipeline(paths))),
            session_lock: Arc::new(Mutex::new(())),
            queue: RagIndexQueue::open(paths.data_dir.join(RAG_INDEX_QUEUE_FILE))?,
            store,
            worker_id: Arc::from(format!("rag-worker-{}", uuid::Uuid::new_v4())),
            worker_started: Arc::new(AtomicBool::new(false)),
            worker_notify: Arc::new(Notify::new()),
            scope_epoch: Arc::new(AtomicU64::new(0)),
            scope_changed: Arc::new(Notify::new()),
            paths: paths.clone(),
        };
        coordinator.ensure_worker();
        Ok(coordinator)
    }

    pub(crate) fn refresh_from_paths(&self, paths: &AppPaths) -> bool {
        let desired_scope = match current_rag_scope(paths) {
            Ok(scope) => scope,
            Err(error) => {
                warn!(error = %error, "failed to resolve current RAG account scope");
                None
            }
        };
        let existing_scope = match self.pipeline.read() {
            Ok(guard) => guard.as_ref().map(|pipeline| pipeline.scope().clone()),
            Err(_) => {
                warn!("failed to acquire RAG pipeline lock for refresh");
                return false;
            }
        };
        if existing_scope == desired_scope {
            return false;
        }

        let mut replacement = desired_scope
            .as_ref()
            .and_then(|_| init_rag_pipeline(paths));
        let confirmed_scope = current_rag_scope(paths).ok().flatten();
        if replacement.as_ref().map(|pipeline| pipeline.scope()) != confirmed_scope.as_ref() {
            replacement = None;
        };

        let Ok(mut guard) = self.pipeline.write() else {
            warn!("failed to acquire RAG pipeline lock for refresh");
            return false;
        };
        let stored_scope = guard.as_ref().map(|pipeline| pipeline.scope().clone());
        let replacement_scope = replacement
            .as_ref()
            .map(|pipeline| pipeline.scope().clone());
        if stored_scope == replacement_scope {
            return false;
        }
        *guard = replacement;
        drop(guard);
        self.scope_epoch.fetch_add(1, Ordering::AcqRel);
        self.scope_changed.notify_waiters();
        if replacement_scope.is_some() {
            info!("RAG pipeline refreshed for current account scope");
        } else {
            info!("RAG pipeline cleared after authentication change");
        }
        self.ensure_worker();
        self.worker_notify.notify_one();
        true
    }

    pub(crate) fn index_transcript(&self, _store: MeetingStore, session_id: String, text: String) {
        if text.trim().is_empty() {
            return;
        }
        self.enqueue_session_rebuild(&session_id, "transcript_final");
    }

    pub(crate) fn index_context_artifacts(
        &self,
        _store: MeetingStore,
        session_id: String,
        artifacts: Vec<ContextArtifact>,
    ) {
        if artifacts.is_empty() {
            return;
        }
        self.enqueue_session_rebuild(&session_id, "context_ready");
    }

    pub(crate) fn reindex_meeting(&self, _store: MeetingStore, meeting: MeetingRecord) {
        let Some(rag) = self.pipeline() else {
            return;
        };
        if !meeting_belongs_to_rag_scope(&meeting, rag.scope()) {
            warn!(
                session_id = %meeting.id,
                "skipping RAG rebuild enqueue for a session outside the current owner scope"
            );
            return;
        }
        self.enqueue_meeting_rebuild(&meeting, rag.scope(), "session_reindex");
    }

    pub(crate) fn delete_session(
        &self,
        session_id: String,
        session_owner_account_id: Option<String>,
    ) {
        let pipeline = self.pipeline();
        let scope = pipeline
            .as_ref()
            .map(|rag| rag.scope().clone())
            .or_else(|| {
                deletion_scope_for_session_owner(&self.paths, session_owner_account_id.as_deref())
            });
        let Some(scope) = scope else {
            // A bare session ID is not sufficient authority to remove
            // tenant-scoped rows when the saved meeting owner cannot be
            // matched to a concrete local RAG scope.
            return;
        };
        if !owner_account_matches_rag_scope(session_owner_account_id.as_deref(), &scope) {
            warn!(
                session_id = %session_id,
                "refusing RAG deletion outside the current owner scope"
            );
            return;
        }
        if let Err(error) = self.queue.cancel_session(&scope, &session_id) {
            warn!(
                session_id = %session_id,
                error_code = rag_queue_error_code(&error),
                "failed to cancel deleted session RAG queue metadata"
            );
        }

        if let Some(rag) = pipeline {
            let session_lock = Arc::clone(&self.session_lock);
            tokio::spawn(async move {
                let _guard = session_lock.lock().await;
                if rag.delete_session(&session_id).await.is_err() {
                    warn!(
                        session_id = %session_id,
                        "failed to clear deleted session RAG index"
                    );
                }
            });
        } else {
            let store_path = self.paths.data_dir.join("rag_vectors.db");
            if !store_path.exists() {
                return;
            }
            tokio::task::spawn_blocking(move || {
                let result =
                    cue_rag::VectorStore::delete_session_at_path(&store_path, &scope, &session_id);
                if result.is_err() {
                    warn!(
                        session_id = %session_id,
                        "failed to clear deleted session RAG index without an active embedder"
                    );
                }
            });
        }
    }

    pub(crate) async fn query_current_and_global(
        &self,
        query_text: &str,
        current_limit: usize,
        current_session_id: &str,
        global_limit: usize,
    ) -> Result<(Vec<cue_rag::RagHit>, Vec<cue_rag::RagHit>)> {
        let query_scope_epoch = self.scope_epoch.load(Ordering::Acquire);
        let Some(rag) = self.pipeline() else {
            return Ok((Vec::new(), Vec::new()));
        };
        if self.scope_epoch.load(Ordering::Acquire) != query_scope_epoch {
            return Ok((Vec::new(), Vec::new()));
        }
        let query_scope = rag.scope().clone();
        let query = rag.query_current_and_global(
            query_text,
            current_limit,
            current_session_id,
            global_limit,
        );
        tokio::pin!(query);
        let results = tokio::select! {
            biased;
            _ = wait_for_scope_epoch_change(
                Arc::clone(&self.scope_epoch),
                Arc::clone(&self.scope_changed),
                query_scope_epoch,
            ) => {
                debug!("cancelled RAG query after account scope changed");
                return Ok((Vec::new(), Vec::new()));
            }
            result = &mut query => result?,
        };
        if current_rag_scope(&self.paths).ok().flatten().as_ref() != Some(&query_scope) {
            self.refresh_from_paths(&self.paths);
            debug!("discarding RAG query results after account scope changed");
            return Ok((Vec::new(), Vec::new()));
        }
        Ok(results)
    }

    fn pipeline(&self) -> Option<Arc<crate::db::rag::RagPipeline>> {
        self.refresh_from_paths(&self.paths);
        match self.pipeline.read() {
            Ok(guard) => guard.clone(),
            Err(_) => {
                warn!("failed to acquire RAG pipeline lock");
                None
            }
        }
    }

    fn enqueue_session_rebuild(&self, session_id: &str, reason: &'static str) {
        let Some(rag) = self.pipeline() else {
            return;
        };
        let Ok(session_uuid) = uuid::Uuid::parse_str(session_id) else {
            warn!(reason, "skipping RAG enqueue for invalid session id");
            return;
        };
        match self.store.load_by_id(session_uuid) {
            Ok(Some(meeting)) if meeting_belongs_to_rag_scope(&meeting, rag.scope()) => {
                self.enqueue_meeting_rebuild(&meeting, rag.scope(), reason);
            }
            Ok(Some(_)) => warn!(
                session_id,
                reason, "skipping RAG enqueue for a session outside the current owner scope"
            ),
            Ok(None) => debug!(
                session_id,
                reason, "skipping RAG enqueue for deleted session"
            ),
            Err(error) => warn!(
                session_id,
                reason,
                error_code = meeting_load_error_code(&error),
                "failed to load session before RAG enqueue"
            ),
        }
    }

    fn enqueue_meeting_rebuild(
        &self,
        meeting: &MeetingRecord,
        scope: &RagScope,
        reason: &'static str,
    ) {
        let revision = match meeting_index_revision(meeting) {
            Ok(revision) => revision,
            Err(error) => {
                warn!(
                    session_id = %meeting.id,
                    reason,
                    error_code = rag_queue_error_code(&error),
                    "failed to compute RAG session revision"
                );
                return;
            }
        };
        match self
            .queue
            .enqueue(scope, &meeting.id.to_string(), &revision, queue_now_ms())
        {
            Ok(changed) => {
                if changed {
                    debug!(
                        session_id = %meeting.id,
                        reason,
                        revision = %short_revision(&revision),
                        "queued durable RAG session rebuild"
                    );
                }
                self.ensure_worker();
                self.worker_notify.notify_one();
            }
            Err(error) => warn!(
                session_id = %meeting.id,
                reason,
                error_code = rag_queue_error_code(&error),
                "failed to queue durable RAG session rebuild"
            ),
        }
    }

    fn ensure_worker(&self) {
        if self
            .worker_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.worker_started.store(false, Ordering::Release);
            return;
        };
        let coordinator = self.clone();
        runtime.spawn(async move {
            coordinator.worker_loop().await;
        });
    }

    async fn worker_loop(self) {
        loop {
            match self.process_next_job().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => warn!(
                    error_code = rag_queue_error_code(&error),
                    "RAG index worker cycle failed"
                ),
            }
            tokio::select! {
                _ = self.worker_notify.notified() => {}
                _ = tokio::time::sleep(RAG_INDEX_IDLE_POLL) => {}
            }
        }
    }

    async fn process_next_job(&self) -> Result<bool> {
        let claimed_scope_epoch = self.scope_epoch.load(Ordering::Acquire);
        let Some(rag) = self.pipeline() else {
            return Ok(false);
        };
        if self.scope_epoch.load(Ordering::Acquire) != claimed_scope_epoch {
            return Ok(true);
        }
        let Some(job) = self.queue.claim_next(
            rag.scope(),
            self.worker_id.as_ref(),
            queue_now_ms(),
            RAG_INDEX_LEASE_TTL,
        )?
        else {
            return Ok(false);
        };
        if self.scope_epoch.load(Ordering::Acquire) != claimed_scope_epoch {
            self.queue
                .fail(&job, "scope_changed", true, queue_now_ms())?;
            return Ok(true);
        }

        let _guard = self.session_lock.lock().await;
        let session_uuid = match uuid::Uuid::parse_str(&job.session_id) {
            Ok(id) => id,
            Err(_) => {
                self.queue
                    .fail(&job, "invalid_session_id", false, queue_now_ms())?;
                return Ok(true);
            }
        };
        let meeting = match self.store.load_by_id(session_uuid) {
            Ok(Some(meeting)) => meeting,
            Ok(None) => {
                let _ = rag.delete_session(&job.session_id).await;
                self.queue.cancel_session(rag.scope(), &job.session_id)?;
                return Ok(true);
            }
            Err(_) => {
                self.queue
                    .fail(&job, "meeting_load_failed", true, queue_now_ms())?;
                return Ok(true);
            }
        };
        if !meeting_belongs_to_rag_scope(&meeting, rag.scope()) {
            self.queue
                .fail(&job, "owner_scope_mismatch", false, queue_now_ms())?;
            return Ok(true);
        }

        let current_revision = meeting_index_revision(&meeting)?;
        if current_revision != job.claimed_revision {
            self.queue.enqueue(
                rag.scope(),
                &job.session_id,
                &current_revision,
                queue_now_ms(),
            )?;
            self.queue.complete(&job, queue_now_ms())?;
            return Ok(true);
        }

        let rebuild = rebuild_meeting_rag_index(Arc::clone(&rag), &meeting);
        tokio::pin!(rebuild);
        let rebuild_result = tokio::select! {
            biased;
            _ = wait_for_scope_epoch_change(
                Arc::clone(&self.scope_epoch),
                Arc::clone(&self.scope_changed),
                claimed_scope_epoch,
            ) => {
                self.queue
                    .fail(&job, "scope_changed", true, queue_now_ms())?;
                debug!(
                    session_id = %job.session_id,
                    "cancelled RAG rebuild after account scope changed"
                );
                return Ok(true);
            }
            result = &mut rebuild => result,
        };
        if let Err(error) = rebuild_result {
            let error_code = rag_rebuild_error_code(&error);
            self.queue.fail(&job, error_code, true, queue_now_ms())?;
            warn!(
                session_id = %job.session_id,
                revision = %short_revision(&job.claimed_revision),
                attempt = job.attempts,
                error_code,
                "durable RAG session rebuild failed"
            );
            return Ok(true);
        }

        // Re-read after indexing. If content changed during the rebuild, keep
        // the completed snapshot only temporarily and immediately queue the
        // new revision; completion below will transition back to pending.
        match self.store.load_by_id(session_uuid) {
            Ok(Some(latest)) if meeting_belongs_to_rag_scope(&latest, rag.scope()) => {
                let latest_revision = meeting_index_revision(&latest)?;
                if latest_revision != job.claimed_revision {
                    self.queue.enqueue(
                        rag.scope(),
                        &job.session_id,
                        &latest_revision,
                        queue_now_ms(),
                    )?;
                }
            }
            Ok(Some(_)) => {
                self.queue
                    .fail(&job, "owner_scope_changed", false, queue_now_ms())?;
                return Ok(true);
            }
            Ok(None) => {
                let _ = rag.delete_session(&job.session_id).await;
                self.queue.cancel_session(rag.scope(), &job.session_id)?;
                return Ok(true);
            }
            Err(_) => {
                self.queue
                    .fail(&job, "meeting_reload_failed", true, queue_now_ms())?;
                return Ok(true);
            }
        }
        self.queue.complete(&job, queue_now_ms())?;
        info!(
            session_id = %job.session_id,
            revision = %short_revision(&job.claimed_revision),
            attempt = job.attempts,
            "durable RAG session rebuild completed"
        );
        Ok(true)
    }
}

fn meeting_belongs_to_rag_scope(meeting: &MeetingRecord, scope: &RagScope) -> bool {
    owner_account_matches_rag_scope(meeting.owner_account_id.as_deref(), scope)
}

fn owner_account_matches_rag_scope(owner_account_id: Option<&str>, scope: &RagScope) -> bool {
    let owner_account_id = owner_account_id
        .map(str::trim)
        .filter(|owner| !owner.is_empty());
    if scope.account_id() == LOCAL_RAG_ACCOUNT_ID {
        owner_account_id.is_none()
    } else {
        owner_account_id == Some(scope.account_id())
    }
}

async fn rebuild_meeting_rag_index(
    rag: Arc<crate::db::rag::RagPipeline>,
    meeting: &MeetingRecord,
) -> Result<()> {
    let session_id = meeting.id.to_string();
    rag.claim_legacy_session(&session_id).await?;
    let staging_session_id = format!(
        "{}{}:{}",
        cue_rag::RAG_STAGING_SESSION_PREFIX,
        session_id,
        uuid::Uuid::new_v4()
    );
    let rebuild =
        index_meeting_rag_generation(Arc::clone(&rag), meeting, &staging_session_id).await;
    if let Err(error) = rebuild {
        let _ = rag.delete_session(&staging_session_id).await;
        return Err(error);
    }
    if let Err(error) = rag
        .replace_session_from_staging(&session_id, &staging_session_id)
        .await
    {
        let _ = rag.delete_session(&staging_session_id).await;
        return Err(error);
    }
    Ok(())
}

async fn index_meeting_rag_generation(
    rag: Arc<crate::db::rag::RagPipeline>,
    meeting: &MeetingRecord,
    session_id: &str,
) -> Result<()> {
    if let Some(summary) = meeting
        .summary
        .as_ref()
        .filter(|summary| !summary.trim().is_empty())
    {
        rag.index_transcript(
            session_id,
            &format!("Compacted session summary:\n{}", summary.trim()),
        )
        .await?;
    }

    for epoch in &meeting.conversation_memory.epochs {
        if !epoch.summary.trim().is_empty() {
            rag.index_transcript(
                session_id,
                &format!(
                    "Compacted conversation memory revision {} ({} earlier turns):\n{}",
                    epoch.revision,
                    epoch.turn_count,
                    epoch.summary.trim()
                ),
            )
            .await?;
        }
    }

    for turn in &meeting.conversation {
        let question = turn.question.trim();
        let answer = turn.answer.trim();
        if !question.is_empty() || !answer.is_empty() {
            rag.index_transcript(
                session_id,
                &format!("Prior Bluey answer\nQuestion: {question}\nAnswer: {answer}"),
            )
            .await?;
        }
    }

    for segment in &meeting.transcript {
        if segment.is_final && !segment.text.trim().is_empty() {
            rag.index_transcript(session_id, &segment.text).await?;
        }
    }
    for artifact in &meeting.context {
        rag.index_context_artifact(session_id, artifact).await?;
    }
    Ok(())
}

fn meeting_index_revision(meeting: &MeetingRecord) -> Result<String> {
    let final_transcript = meeting
        .transcript
        .iter()
        .filter(|segment| segment.is_final && !segment.text.trim().is_empty())
        .collect::<Vec<_>>();
    let ready_context = meeting
        .context
        .iter()
        .filter(|artifact| artifact.processing_status == cue_core::ContextProcessingStatus::Ready)
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "session_id": meeting.id,
        "owner_account_id": &meeting.owner_account_id,
        "summary": &meeting.summary,
        "conversation_memory": &meeting.conversation_memory,
        "conversation": &meeting.conversation,
        "final_transcript": final_transcript,
        "ready_context": ready_context,
    }))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

async fn wait_for_scope_epoch_change(
    scope_epoch: Arc<AtomicU64>,
    scope_changed: Arc<Notify>,
    claimed_epoch: u64,
) {
    loop {
        let notified = scope_changed.notified();
        if scope_epoch.load(Ordering::Acquire) != claimed_epoch {
            return;
        }
        notified.await;
    }
}

fn queue_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn short_revision(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

fn rag_rebuild_error_code(error: &anyhow::Error) -> &'static str {
    let lower = format!("{error:#}").to_ascii_lowercase();
    if lower.contains("embedding") {
        "embedding_failed"
    } else if lower.contains("artifact text") {
        "artifact_read_failed"
    } else if lower.contains("store rag") || lower.contains("vector") || lower.contains("sqlite") {
        "vector_store_failed"
    } else {
        "index_rebuild_failed"
    }
}

fn rag_queue_error_code(error: &anyhow::Error) -> &'static str {
    let lower = format!("{error:#}").to_ascii_lowercase();
    if lower.contains("database is locked") || lower.contains("busy") {
        "queue_busy"
    } else if lower.contains("permission") {
        "queue_permission"
    } else if lower.contains("symlink") {
        "queue_path_rejected"
    } else {
        "queue_storage_failed"
    }
}

fn meeting_load_error_code(error: &anyhow::Error) -> &'static str {
    let lower = format!("{error:#}").to_ascii_lowercase();
    if lower.contains("parse") {
        "meeting_parse_failed"
    } else if lower.contains("permission") {
        "meeting_permission_failed"
    } else {
        "meeting_load_failed"
    }
}

/// Initialize the RAG pipeline if cloud processing is explicitly enabled and
/// a managed Bluey account is linked.
///
/// Production/customer installs use `/router/embed`, so provider API keys stay
/// on `bluey-server`. Direct OpenAI embedding remains an explicit development
/// fallback only.
fn init_rag_pipeline(paths: &AppPaths) -> Option<Arc<crate::db::rag::RagPipeline>> {
    match cloud_embedding_consent_active(paths) {
        Ok(true) => {}
        Ok(false) => {
            info!(
                "RAG indexing deferred locally: cloud sync and cloud consent are not both enabled"
            );
            return None;
        }
        Err(error) => {
            warn!(
                error = %error,
                "RAG indexing deferred locally because persisted cloud consent could not be read"
            );
            return None;
        }
    }
    let scope = match current_rag_scope(paths) {
        Ok(Some(scope)) => scope,
        Ok(None) => {
            info!("RAG pipeline disabled: link a Bluey account for managed embeddings");
            return None;
        }
        Err(error) => {
            warn!("RAG account scope unavailable: {error:#}");
            return None;
        }
    };
    let embedder = if scope.account_id() == LOCAL_RAG_ACCOUNT_ID {
        dev_openai_embedder()?
    } else {
        match managed_embedder(paths) {
            Ok(Some(embedder)) => embedder,
            Ok(None) => return None,
            Err(error) => {
                warn!("managed RAG embedder unavailable: {error:#}");
                return None;
            }
        }
    };
    if current_rag_scope(paths).ok().flatten().as_ref() != Some(&scope) {
        debug!("RAG account scope changed during pipeline initialization");
        return None;
    }
    let store_path = paths.data_dir.join("rag_vectors.db");
    match crate::db::rag::RagPipeline::new(store_path, embedder, scope) {
        Ok(pipeline) => {
            info!("RAG pipeline initialized");
            Some(Arc::new(pipeline))
        }
        Err(e) => {
            warn!("RAG pipeline init failed: {e:#}");
            None
        }
    }
}

fn current_rag_scope(paths: &AppPaths) -> anyhow::Result<Option<RagScope>> {
    if !cloud_embedding_consent_active(paths)? {
        return Ok(None);
    }
    if let Some(scope) = managed_account_scope(paths)? {
        return Ok(Some(scope));
    }
    if dev_openai_api_key().is_some() {
        return Ok(Some(local_rag_scope()));
    }
    Ok(None)
}

fn deletion_scope_for_session_owner(
    paths: &AppPaths,
    session_owner_account_id: Option<&str>,
) -> Option<RagScope> {
    let Some(owner_account_id) = session_owner_account_id else {
        return Some(local_rag_scope());
    };
    let account = load_account(paths).ok().flatten()?;
    let account_id = account
        .cloud_account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let user_id = account.user_id.trim();
            (!user_id.is_empty() && user_id != "local-user").then_some(user_id)
        })?;
    if account_id != owner_account_id {
        return None;
    }
    RagScope::new(account_id, Some(account.workspace_id.as_str())).ok()
}

fn managed_account_scope(paths: &AppPaths) -> anyhow::Result<Option<RagScope>> {
    let Some(account) = load_account(paths)? else {
        return Ok(None);
    };
    if account.provider.trim() != "bluey" {
        return Ok(None);
    }
    let Some(tokens) = cue_cloud_client::SecureAccountStore::new(paths.clone()).load()? else {
        return Ok(None);
    };
    let account_user_id = account.user_id.trim();
    let token_user_id = tokens.email.trim();
    anyhow::ensure!(
        account_user_id.is_empty() || token_user_id.is_empty() || account_user_id == token_user_id,
        "RAG account profile does not match stored credentials"
    );
    Ok(Some(rag_scope_for_account(&account)?))
}

fn rag_scope_for_account(account: &cue_core::AccountConfig) -> anyhow::Result<RagScope> {
    let account_id = account
        .cloud_account_id
        .as_deref()
        .map(str::trim)
        .filter(|account_id| !account_id.is_empty())
        .or_else(|| {
            let user_id = account.user_id.trim();
            (!user_id.is_empty() && user_id != "local-user").then_some(user_id)
        })
        .ok_or_else(|| anyhow::anyhow!("linked Bluey account has no stable account id"))?;
    RagScope::new(account_id, Some(account.workspace_id.as_str()))
}

fn local_rag_scope() -> RagScope {
    RagScope::new(LOCAL_RAG_ACCOUNT_ID, Some("default"))
        .expect("local RAG scope constants must be valid")
}

fn managed_embedder(paths: &AppPaths) -> anyhow::Result<Option<Arc<dyn EmbeddingProvider>>> {
    if !cloud_embedding_consent_active(paths)? {
        return Ok(None);
    }
    let Some(account) = load_account(paths)? else {
        return Ok(None);
    };
    if account.provider.trim() != "bluey" || managed_account_scope(paths)?.is_none() {
        return Ok(None);
    }

    let base_url = std::env::var("BLUEY_CLOUD_API_URL")
        .or_else(|_| std::env::var("CUE_CLOUD_API_URL"))
        .unwrap_or_else(|_| account.api_url.clone());
    let config = cue_cloud_client::client::ClientConfig {
        base_url,
        ..Default::default()
    };
    let client = cue_cloud_client::CloudClient::new(
        config,
        Arc::new(cue_cloud_client::SecureAccountStore::new(paths.clone())),
    )?;
    if client.current_tokens().is_none() {
        return Ok(None);
    }
    info!("RAG embeddings configured through Bluey managed router");
    Ok(Some(Arc::new(ManagedBlueyEmbedder::new(
        client,
        paths.clone(),
    ))))
}

fn cloud_embedding_consent_active(paths: &AppPaths) -> anyhow::Result<bool> {
    Ok(load_settings(paths)?.cloud_sync_allowed())
}

fn ensure_managed_embedding_consent(paths: &AppPaths) -> std::result::Result<(), EmbeddingError> {
    match cloud_embedding_consent_active(paths) {
        Ok(true) => Ok(()),
        Ok(false) => Err(EmbeddingError::Request(
            "managed embedding is disabled until cloud sync and cloud consent are enabled"
                .to_string(),
        )),
        Err(_) => Err(EmbeddingError::Request(
            "managed embedding is disabled because persisted cloud consent is unavailable"
                .to_string(),
        )),
    }
}

fn dev_openai_embedder() -> Option<Arc<dyn EmbeddingProvider>> {
    dev_openai_api_key().map(|api_key| {
        warn!("RAG using direct OpenAI embeddings from local developer configuration");
        Arc::new(cue_rag::embedder::OpenAiEmbedder::new(api_key)) as Arc<dyn EmbeddingProvider>
    })
}

fn dev_openai_api_key() -> Option<String> {
    if !dev_env_truthy("BLUEY_DEV_BYOK") {
        return None;
    }
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| crate::secrets::load_api_key("openai").ok().flatten())
}

fn bounded_embed_input(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_MANAGED_EMBED_INPUT_CHARS {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(MAX_MANAGED_EMBED_INPUT_CHARS)
        .collect()
}

fn map_cloud_embed_error(error: cue_cloud_client::Error) -> EmbeddingError {
    match error {
        cue_cloud_client::Error::Unauthorized => EmbeddingError::NoApiKey,
        cue_cloud_client::Error::InsufficientBalance { .. } => {
            EmbeddingError::Request("Bluey account balance is too low for RAG embeddings".into())
        }
        cue_cloud_client::Error::TrialEnded => {
            EmbeddingError::Request("Bluey trial ended before RAG embedding".into())
        }
        cue_cloud_client::Error::RateLimited { retry_after_secs } => EmbeddingError::Request(
            format!("Bluey embedding rate limited; retry after {retry_after_secs}s"),
        ),
        cue_cloud_client::Error::CapacityBusy {
            retry_after_secs, ..
        } => EmbeddingError::Request(format!(
            "Bluey embedding capacity busy; retry after {retry_after_secs}s"
        )),
        cue_cloud_client::Error::Server { status } => {
            EmbeddingError::Request(format!("Bluey embedding server error {status}"))
        }
        cue_cloud_client::Error::InternalDisclosureBlocked => {
            EmbeddingError::Request("Bluey embedding request was blocked".into())
        }
        cue_cloud_client::Error::Network(_) => {
            EmbeddingError::Request("Bluey embedding network error".into())
        }
        cue_cloud_client::Error::Json(_) => {
            EmbeddingError::InvalidResponse("Bluey embedding response was invalid".into())
        }
        cue_cloud_client::Error::TokenStore(_) => {
            EmbeddingError::Request("Bluey account token store error".into())
        }
        cue_cloud_client::Error::Other(_) => {
            EmbeddingError::Request("Bluey embedding request failed".into())
        }
    }
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn dev_env_truthy(name: &str) -> bool {
    cfg!(debug_assertions) && env_truthy(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::{save_account, save_settings, AccountConfig, CueSettings};
    use std::sync::Mutex;

    static PLAINTEXT_TOKEN_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_plaintext_token_fallback<T>(f: impl FnOnce() -> T) -> T {
        let _guard = PLAINTEXT_TOKEN_TEST_LOCK.lock().unwrap();
        let old = std::env::var_os("BLUEY_ALLOW_PLAINTEXT_TOKENS");
        std::env::set_var("BLUEY_ALLOW_PLAINTEXT_TOKENS", "1");
        let result = f();
        match old {
            Some(value) => std::env::set_var("BLUEY_ALLOW_PLAINTEXT_TOKENS", value),
            None => std::env::remove_var("BLUEY_ALLOW_PLAINTEXT_TOKENS"),
        }
        result
    }

    fn test_paths() -> AppPaths {
        let base = std::env::temp_dir().join(format!("bluey-managed-rag-{}", uuid::Uuid::new_v4()));
        AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("runtime"),
            state_file: base.join("runtime/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        }
    }

    fn linked_account(account_id: &str, user_id: &str, workspace_id: &str) -> AccountConfig {
        let mut account = AccountConfig::local();
        account.provider = "bluey".to_string();
        account.api_url = "http://127.0.0.1:8787".to_string();
        account.cloud_account_id = Some(account_id.to_string());
        account.user_id = user_id.to_string();
        account.workspace_id = workspace_id.to_string();
        account.access_token = Some(format!("access-{account_id}"));
        account.refresh_token = Some(format!("refresh-{account_id}"));
        account
    }

    fn save_cloud_consent(paths: &AppPaths, enabled: bool, consent_granted: bool) {
        save_settings(
            paths,
            &CueSettings {
                cloud_sync_enabled: enabled,
                cloud_sync_consent_granted: consent_granted,
                ..CueSettings::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn managed_embedder_requires_linked_account_token() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            save_cloud_consent(&paths, true, true);
            assert!(managed_embedder(&paths).unwrap().is_none());

            let mut account = AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = "http://127.0.0.1:8787".to_string();
            account.user_id = "tester@bluey.sh".to_string();
            save_account(&paths, &account).unwrap();
            assert!(managed_embedder(&paths).unwrap().is_none());
        });
    }

    #[test]
    fn managed_embedder_uses_account_file_tokens_without_provider_key() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            let mut account = AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = "http://127.0.0.1:8787".to_string();
            account.user_id = "tester@bluey.sh".to_string();
            account.access_token = Some("desktop-access-token".to_string());
            account.refresh_token = Some("desktop-refresh-token".to_string());
            save_account(&paths, &account).unwrap();
            save_cloud_consent(&paths, true, true);

            let embedder = managed_embedder(&paths).unwrap().expect("managed embedder");
            assert_eq!(embedder.name(), "bluey-managed");
            assert_eq!(embedder.dim(), MANAGED_EMBED_DIM);
        });
    }

    #[test]
    fn managed_embedder_requires_both_persisted_cloud_switches() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            save_account(
                &paths,
                &linked_account("account-a", "a@example.com", "workspace-a"),
            )
            .unwrap();

            assert!(managed_embedder(&paths).unwrap().is_none());
            assert!(current_rag_scope(&paths).unwrap().is_none());

            save_cloud_consent(&paths, true, false);
            assert!(managed_embedder(&paths).unwrap().is_none());
            assert!(current_rag_scope(&paths).unwrap().is_none());

            save_cloud_consent(&paths, false, true);
            assert!(managed_embedder(&paths).unwrap().is_none());
            assert!(current_rag_scope(&paths).unwrap().is_none());

            save_cloud_consent(&paths, true, true);
            let embedder = managed_embedder(&paths).unwrap().expect("managed embedder");
            assert_eq!(embedder.name(), "bluey-managed");
            assert_eq!(
                current_rag_scope(&paths)
                    .unwrap()
                    .expect("managed scope")
                    .account_id(),
                "account-a"
            );
        });
    }

    #[test]
    fn retained_managed_embedder_rechecks_revoked_consent_before_network_use() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            let mut account = linked_account("account-a", "a@example.com", "workspace-a");
            account.api_url = "http://127.0.0.1:9".to_string();
            save_account(&paths, &account).unwrap();
            save_cloud_consent(&paths, true, true);
            let embedder = managed_embedder(&paths).unwrap().expect("managed embedder");

            save_cloud_consent(&paths, false, false);
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let error = runtime
                .block_on(embedder.embed("must remain local"))
                .expect_err("revoked consent must stop the retained provider");
            assert!(error.to_string().contains("cloud consent"));
        });
    }

    #[test]
    fn bounded_embed_input_trims_and_caps_text() {
        assert_eq!(bounded_embed_input("  hello  "), "hello");
        let long = "x".repeat(MAX_MANAGED_EMBED_INPUT_CHARS + 100);
        assert_eq!(
            bounded_embed_input(&long).chars().count(),
            MAX_MANAGED_EMBED_INPUT_CHARS
        );
    }

    #[tokio::test]
    async fn scope_epoch_waiter_cancels_work_after_account_change() {
        let epoch = Arc::new(AtomicU64::new(7));
        let changed = Arc::new(Notify::new());
        let waiter = tokio::spawn(wait_for_scope_epoch_change(
            Arc::clone(&epoch),
            Arc::clone(&changed),
            7,
        ));
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        epoch.fetch_add(1, Ordering::AcqRel);
        changed.notify_waiters();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("scope-change waiter should wake")
            .expect("scope-change waiter task should succeed");
    }

    #[test]
    fn coordinator_refreshes_after_account_link() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            paths.ensure().unwrap();
            let store = MeetingStore::new(&paths).unwrap();
            let coordinator = RagIndexCoordinator::from_paths(&paths, store).unwrap();
            assert!(coordinator.pipeline().is_none());
            assert!(!coordinator.refresh_from_paths(&paths));

            let mut account = AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = "http://127.0.0.1:8787".to_string();
            account.user_id = "tester@bluey.sh".to_string();
            account.access_token = Some("desktop-access-token".to_string());
            account.refresh_token = Some("desktop-refresh-token".to_string());
            save_account(&paths, &account).unwrap();
            save_cloud_consent(&paths, true, true);

            assert!(coordinator.refresh_from_paths(&paths));
            assert!(coordinator.pipeline().is_some());
            assert!(!coordinator.refresh_from_paths(&paths));
        });
    }

    #[test]
    fn coordinator_swaps_pipeline_on_account_change_and_clears_on_logout() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            paths.ensure().unwrap();
            save_account(
                &paths,
                &linked_account("account-a", "a@example.com", "workspace-a"),
            )
            .unwrap();
            save_cloud_consent(&paths, true, true);

            let store = MeetingStore::new(&paths).unwrap();
            let coordinator = RagIndexCoordinator::from_paths(&paths, store).unwrap();
            let pipeline_a = coordinator.pipeline().expect("account A pipeline");
            assert_eq!(pipeline_a.scope().account_id(), "account-a");
            assert_eq!(pipeline_a.scope().workspace_id(), Some("workspace-a"));

            save_account(
                &paths,
                &linked_account("account-b", "b@example.com", "workspace-b"),
            )
            .unwrap();
            let pipeline_b = coordinator.pipeline().expect("account B pipeline");
            assert_eq!(pipeline_b.scope().account_id(), "account-b");
            assert_eq!(pipeline_b.scope().workspace_id(), Some("workspace-b"));
            assert!(!Arc::ptr_eq(&pipeline_a, &pipeline_b));

            let mut signed_out = linked_account("account-b", "b@example.com", "workspace-b");
            signed_out.access_token = None;
            signed_out.refresh_token = None;
            save_account(&paths, &signed_out).unwrap();
            assert!(coordinator.pipeline().is_none());
        });
    }

    #[test]
    fn coordinator_applies_cloud_consent_grant_and_revocation() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            paths.ensure().unwrap();
            save_account(
                &paths,
                &linked_account("account-a", "a@example.com", "workspace-a"),
            )
            .unwrap();
            save_cloud_consent(&paths, false, false);

            let store = MeetingStore::new(&paths).unwrap();
            let coordinator = RagIndexCoordinator::from_paths(&paths, store).unwrap();
            assert!(coordinator.pipeline().is_none());

            save_cloud_consent(&paths, true, true);
            assert!(coordinator.refresh_from_paths(&paths));
            assert_eq!(
                coordinator
                    .pipeline()
                    .expect("pipeline after consent grant")
                    .scope()
                    .account_id(),
                "account-a"
            );

            save_cloud_consent(&paths, true, false);
            assert!(coordinator.refresh_from_paths(&paths));
            assert!(coordinator.pipeline().is_none());
            assert!(!coordinator.refresh_from_paths(&paths));
        });
    }

    #[test]
    fn meeting_ownership_must_match_pipeline_account() {
        let mut meeting = MeetingRecord::new(None);
        let account_a = RagScope::new("account-a", Some("workspace-a")).unwrap();
        let account_b = RagScope::new("account-b", Some("workspace-b")).unwrap();

        assert!(meeting_belongs_to_rag_scope(&meeting, &local_rag_scope()));
        assert!(!meeting_belongs_to_rag_scope(&meeting, &account_a));

        meeting.owner_account_id = Some("account-a".to_string());
        assert!(meeting_belongs_to_rag_scope(&meeting, &account_a));
        assert!(!meeting_belongs_to_rag_scope(&meeting, &account_b));
        assert!(!meeting_belongs_to_rag_scope(&meeting, &local_rag_scope()));
    }

    #[test]
    fn compacted_conversation_memory_changes_rag_revision() {
        let mut meeting = MeetingRecord::new(Some("Long interview".to_string()));
        let before = meeting_index_revision(&meeting).expect("initial revision");
        for index in 0..81 {
            meeting.push_conversation_turn(cue_core::ConversationTurn::new(
                format!("Question {index}"),
                format!("Answer {index}"),
                None,
                Some("test".to_string()),
            ));
        }

        assert!(!meeting.conversation_memory.is_empty());
        let after = meeting_index_revision(&meeting).expect("memory revision");
        assert_ne!(before, after);
    }
}
