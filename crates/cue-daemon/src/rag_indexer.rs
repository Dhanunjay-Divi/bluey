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

use crate::db::rag_queue::{RagIndexQueue, RagVectorDeleteJob};
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
    owner_account_id: String,
    credential_generation: u64,
}

impl ManagedBlueyEmbedder {
    fn new(
        client: cue_cloud_client::CloudClient,
        paths: AppPaths,
        owner_account_id: String,
        credential_generation: u64,
    ) -> Self {
        Self {
            client,
            paths,
            owner_account_id,
            credential_generation,
        }
    }

    fn ensure_current_and_consented(&self) -> std::result::Result<(), EmbeddingError> {
        ensure_managed_embedding_context(
            &self.paths,
            &self.owner_account_id,
            self.credential_generation,
        )
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
        self.ensure_current_and_consented()?;
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
        self.ensure_current_and_consented()?;
        let response = match self
            .client
            .embed_batch(&cue_cloud_client::EmbedBatchRequest {
                request_id: new_request_id(),
                inputs: inputs.clone(),
                model: Some(cue_rag::embedder::OpenAiEmbedder::MODEL.to_string()),
            })
            .await
        {
            Ok(response) => {
                self.ensure_current_and_consented()?;
                response
            }
            Err(cue_cloud_client::Error::Server { status }) if status == 404 || status == 405 => {
                let mut vectors = Vec::with_capacity(inputs.len());
                for input in inputs {
                    self.ensure_current_and_consented()?;
                    let response = self
                        .client
                        .embed(&cue_cloud_client::EmbedRequest {
                            request_id: new_request_id(),
                            input,
                            model: Some(cue_rag::embedder::OpenAiEmbedder::MODEL.to_string()),
                        })
                        .await
                        .map_err(map_cloud_embed_error)?;
                    self.ensure_current_and_consented()?;
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
    tombstone_epoch: Arc<AtomicU64>,
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
            tombstone_epoch: Arc::new(AtomicU64::new(0)),
            paths: paths.clone(),
        };
        coordinator.ensure_worker();
        Ok(coordinator)
    }

    pub(crate) fn refresh_from_paths(&self, paths: &AppPaths) -> bool {
        let mut desired_scope = match current_rag_scope(paths) {
            Ok(scope) => scope,
            Err(error) => {
                warn!(
                    error_code = rag_runtime_error_code(&error),
                    error_ref = %rag_error_ref(&error),
                    "failed to resolve current RAG account scope"
                );
                None
            }
        };
        if desired_scope.as_ref().is_some_and(|scope| {
            self.queue
                .account_is_tombstoned(scope.account_id())
                .unwrap_or(true)
        }) {
            desired_scope = None;
        }
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
                session_ref = %rag_session_ref(&meeting.id.to_string()),
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
                session_ref = %rag_session_ref(&session_id),
                "refusing RAG deletion outside the current owner scope"
            );
            return;
        }
        if let Err(error) = self
            .queue
            .tombstone_session(&scope, &session_id, queue_now_ms())
        {
            warn!(
                session_ref = %rag_session_ref(&session_id),
                error_code = rag_queue_error_code(&error),
                "failed to durably fence deleted session RAG state"
            );
            return;
        }
        self.tombstone_epoch.fetch_add(1, Ordering::AcqRel);
        drop(pipeline);
        self.ensure_worker();
        self.worker_notify.notify_one();
    }

    /// Durably purge RAG state for an account-scoped cloud tombstone.
    ///
    /// Unlike the best-effort UI deletion path, this method waits for any
    /// in-flight rebuild, permanently fences the queue entry, and confirms the
    /// vector deletion before returning.
    pub(crate) async fn purge_cloud_tombstoned_session_for_owner(
        &self,
        owner_account_id: &str,
        session_id: uuid::Uuid,
    ) -> Result<()> {
        let owner_account_id = owner_account_id.trim();
        if owner_account_id.is_empty() {
            anyhow::bail!("cloud RAG deletion requires an account owner");
        }
        let _guard = self.session_lock.lock().await;
        let scope = deletion_scope_for_session_owner(&self.paths, Some(owner_account_id))
            .ok_or_else(|| anyhow::anyhow!("cloud RAG deletion account scope is unavailable"))?;
        if !owner_account_matches_rag_scope(Some(owner_account_id), &scope) {
            anyhow::bail!("cloud RAG deletion account scope changed");
        }
        let session_id = session_id.to_string();
        self.queue
            .tombstone_session(&scope, &session_id, queue_now_ms())?;
        self.tombstone_epoch.fetch_add(1, Ordering::AcqRel);
        let Some(job) = self.queue.claim_vector_delete(
            &scope,
            &session_id,
            self.worker_id.as_ref(),
            queue_now_ms(),
            RAG_INDEX_LEASE_TTL,
        )?
        else {
            if self.queue.vector_delete_is_complete(&scope, &session_id)? {
                return Ok(());
            }
            anyhow::bail!("cloud RAG deletion is already in progress");
        };
        match self.delete_vectors_for_job(&job).await {
            Ok(()) => {
                anyhow::ensure!(
                    self.queue.complete_vector_delete(&job, queue_now_ms())?,
                    "cloud RAG deletion completion lost its lease"
                );
                Ok(())
            }
            Err(error) => {
                self.queue.fail_vector_delete(
                    &job,
                    rag_rebuild_error_code(&error),
                    queue_now_ms(),
                )?;
                self.worker_notify.notify_one();
                Err(error)
            }
        }
    }

    /// Permanently fence and delete every RAG row owned by one deleted
    /// account, including workspaces and orphan queue/vector rows that are no
    /// longer discoverable through MeetingStore or the session projection.
    pub(crate) async fn purge_account_for_owner(&self, owner_account_id: &str) -> Result<usize> {
        let owner_account_id = owner_account_id.trim();
        if owner_account_id.is_empty() {
            anyhow::bail!("RAG account deletion requires an account owner");
        }
        let _guard = self.session_lock.lock().await;
        let removed_metadata = self.queue.purge_account(owner_account_id, queue_now_ms())?;
        self.tombstone_epoch.fetch_add(1, Ordering::AcqRel);
        self.scope_epoch.fetch_add(1, Ordering::AcqRel);
        self.scope_changed.notify_waiters();
        if let Ok(mut pipeline) = self.pipeline.write() {
            if pipeline
                .as_ref()
                .is_some_and(|rag| rag.scope().account_id() == owner_account_id)
            {
                *pipeline = None;
            }
        }
        let store_path = self.paths.data_dir.join("rag_vectors.db");
        let owner = owner_account_id.to_string();
        let deleted_vectors = tokio::task::spawn_blocking(move || {
            cue_rag::VectorStore::delete_account_at_path(&store_path, &owner)
        })
        .await
        .map_err(|_| anyhow::anyhow!("RAG account deletion worker did not complete"))??;
        let remaining = cue_rag::VectorStore::account_chunk_count_at_path(
            &self.paths.data_dir.join("rag_vectors.db"),
            owner_account_id,
        )?;
        anyhow::ensure!(remaining == 0, "RAG account vector purge was incomplete");
        anyhow::ensure!(
            self.queue.account_is_tombstoned(owner_account_id)?,
            "RAG account query fence was not durable"
        );
        Ok(removed_metadata.saturating_add(deleted_vectors))
    }

    pub(crate) async fn query_current_and_global(
        &self,
        query_text: &str,
        current_limit: usize,
        current_session_id: &str,
        global_limit: usize,
    ) -> Result<(Vec<cue_rag::RagHit>, Vec<cue_rag::RagHit>)> {
        let query_scope_epoch = self.scope_epoch.load(Ordering::Acquire);
        let query_tombstone_epoch = self.tombstone_epoch.load(Ordering::Acquire);
        let Some(rag) = self.pipeline() else {
            return Ok((Vec::new(), Vec::new()));
        };
        if self
            .queue
            .account_is_tombstoned(rag.scope().account_id())
            .unwrap_or(true)
        {
            return Ok((Vec::new(), Vec::new()));
        }
        if self.scope_epoch.load(Ordering::Acquire) != query_scope_epoch {
            return Ok((Vec::new(), Vec::new()));
        }
        let query_scope = rag.scope().clone();
        let current_limit = if self
            .queue
            .is_tombstoned(&query_scope, current_session_id)
            .unwrap_or(true)
        {
            0
        } else {
            current_limit
        };
        let query = rag.query_current_and_global(
            query_text,
            current_limit,
            current_session_id,
            global_limit,
        );
        tokio::pin!(query);
        let (mut current, mut global) = tokio::select! {
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
        if self
            .queue
            .account_is_tombstoned(query_scope.account_id())
            .unwrap_or(true)
        {
            return Ok((Vec::new(), Vec::new()));
        }
        remove_tombstoned_hits(&self.queue, &query_scope, &mut current);
        remove_tombstoned_hits(&self.queue, &query_scope, &mut global);
        let observed_tombstone_epoch = self.tombstone_epoch.load(Ordering::Acquire);
        if observed_tombstone_epoch != query_tombstone_epoch {
            remove_tombstoned_hits(&self.queue, &query_scope, &mut current);
            remove_tombstoned_hits(&self.queue, &query_scope, &mut global);
            if self.tombstone_epoch.load(Ordering::Acquire) != observed_tombstone_epoch {
                return Ok((Vec::new(), Vec::new()));
            }
        }
        Ok((current, global))
    }

    fn pipeline(&self) -> Option<Arc<crate::db::rag::RagPipeline>> {
        self.refresh_from_paths(&self.paths);
        match self.pipeline.read() {
            Ok(guard) => guard.clone().filter(|pipeline| {
                !self
                    .queue
                    .account_is_tombstoned(pipeline.scope().account_id())
                    .unwrap_or(true)
            }),
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
                session_ref = %rag_session_ref(session_id),
                reason, "skipping RAG enqueue for a session outside the current owner scope"
            ),
            Ok(None) => debug!(
                session_ref = %rag_session_ref(session_id),
                reason, "skipping RAG enqueue for deleted session"
            ),
            Err(error) => warn!(
                session_ref = %rag_session_ref(session_id),
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
                    session_ref = %rag_session_ref(&meeting.id.to_string()),
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
                        session_ref = %rag_session_ref(&meeting.id.to_string()),
                        reason,
                        revision = %short_revision(&revision),
                        "queued durable RAG session rebuild"
                    );
                }
                self.ensure_worker();
                self.worker_notify.notify_one();
            }
            Err(error) => warn!(
                session_ref = %rag_session_ref(&meeting.id.to_string()),
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
        if self.process_next_vector_delete().await? {
            return Ok(true);
        }
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
                self.queue
                    .tombstone_session(rag.scope(), &job.session_id, queue_now_ms())?;
                self.tombstone_epoch.fetch_add(1, Ordering::AcqRel);
                self.worker_notify.notify_one();
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
                    session_ref = %rag_session_ref(&job.session_id),
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
                session_ref = %rag_session_ref(&job.session_id),
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
                self.queue
                    .tombstone_session(rag.scope(), &job.session_id, queue_now_ms())?;
                self.tombstone_epoch.fetch_add(1, Ordering::AcqRel);
                self.worker_notify.notify_one();
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
            session_ref = %rag_session_ref(&job.session_id),
            revision = %short_revision(&job.claimed_revision),
            attempt = job.attempts,
            "durable RAG session rebuild completed"
        );
        Ok(true)
    }

    async fn process_next_vector_delete(&self) -> Result<bool> {
        let Some(job) = self.queue.claim_next_vector_delete(
            self.worker_id.as_ref(),
            queue_now_ms(),
            RAG_INDEX_LEASE_TTL,
        )?
        else {
            return Ok(false);
        };
        let _guard = self.session_lock.lock().await;
        match self.delete_vectors_for_job(&job).await {
            Ok(()) => {
                anyhow::ensure!(
                    self.queue.complete_vector_delete(&job, queue_now_ms())?,
                    "RAG vector deletion completion lost its lease"
                );
                info!(
                    session_ref = %rag_session_ref(&job.session_id),
                    attempt = job.attempts,
                    "durable RAG vector deletion completed"
                );
            }
            Err(error) => {
                let error_code = rag_rebuild_error_code(&error);
                self.queue
                    .fail_vector_delete(&job, error_code, queue_now_ms())?;
                warn!(
                    session_ref = %rag_session_ref(&job.session_id),
                    attempt = job.attempts,
                    error_code,
                    "durable RAG vector deletion failed and will retry"
                );
            }
        }
        Ok(true)
    }

    async fn delete_vectors_for_job(&self, job: &RagVectorDeleteJob) -> Result<()> {
        let workspace = (!job.workspace_id.is_empty()).then_some(job.workspace_id.as_str());
        let scope = RagScope::new(&job.account_id, workspace)?;
        if let Some(rag) = self
            .pipeline()
            .filter(|pipeline| pipeline.scope() == &scope)
        {
            rag.delete_session(&job.session_id).await?;
            return Ok(());
        }
        let store_path = self.paths.data_dir.join("rag_vectors.db");
        if !store_path.exists() {
            return Ok(());
        }
        let session_id = job.session_id.clone();
        tokio::task::spawn_blocking(move || {
            cue_rag::VectorStore::delete_session_at_path(&store_path, &scope, &session_id)
        })
        .await
        .map_err(|_| anyhow::anyhow!("RAG vector deletion worker did not complete"))??;
        Ok(())
    }
}

fn remove_tombstoned_hits(
    queue: &RagIndexQueue,
    scope: &RagScope,
    hits: &mut Vec<cue_rag::RagHit>,
) {
    hits.retain(|hit| !queue.is_tombstoned(scope, &hit.session_id).unwrap_or(true));
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
        return cleanup_failed_staging(&rag, &staging_session_id, error).await;
    }
    if let Err(error) = rag
        .replace_session_from_staging(&session_id, &staging_session_id)
        .await
    {
        return cleanup_failed_staging(&rag, &staging_session_id, error).await;
    }
    Ok(())
}

async fn cleanup_failed_staging(
    rag: &crate::db::rag::RagPipeline,
    staging_session_id: &str,
    original_error: anyhow::Error,
) -> Result<()> {
    combine_rebuild_and_cleanup_errors(
        original_error,
        rag.delete_session(staging_session_id).await.map(|_| ()),
    )
}

fn combine_rebuild_and_cleanup_errors(
    original_error: anyhow::Error,
    cleanup: Result<()>,
) -> Result<()> {
    cleanup.map_err(|cleanup| {
        anyhow::anyhow!(
            "RAG rebuild failed ({original_error:#}); staging cleanup also failed ({cleanup:#})"
        )
    })?;
    Err(original_error)
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

fn rag_session_ref(session_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-rag-session-log-ref-v1\0");
    digest.update((session_id.len() as u64).to_be_bytes());
    digest.update(session_id.as_bytes());
    hex::encode(&digest.finalize()[..8])
}

fn rag_error_ref(error: &anyhow::Error) -> String {
    let raw = format!("{error:#}");
    let mut digest = Sha256::new();
    digest.update(b"bluey-rag-error-log-ref-v1\0");
    digest.update((raw.len() as u64).to_be_bytes());
    digest.update(raw.as_bytes());
    hex::encode(&digest.finalize()[..8])
}

fn rag_runtime_error_code(error: &anyhow::Error) -> &'static str {
    let lower = format!("{error:#}").to_ascii_lowercase();
    if lower.contains("permission") || lower.contains("access denied") {
        "permission"
    } else if lower.contains("consent") {
        "consent"
    } else if lower.contains("account") || lower.contains("owner") {
        "account_scope"
    } else if lower.contains("sqlite")
        || lower.contains("database")
        || lower.contains("storage")
        || lower.contains("file")
    {
        "storage"
    } else if lower.contains("network") || lower.contains("connect") {
        "network"
    } else {
        "internal"
    }
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
                error_code = rag_runtime_error_code(&error),
                error_ref = %rag_error_ref(&error),
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
            warn!(
                error_code = rag_runtime_error_code(&error),
                error_ref = %rag_error_ref(&error),
                "RAG account scope unavailable"
            );
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
                warn!(
                    error_code = rag_runtime_error_code(&error),
                    error_ref = %rag_error_ref(&error),
                    "managed RAG embedder unavailable"
                );
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
        Err(error) => {
            warn!(
                error_code = rag_runtime_error_code(&error),
                error_ref = %rag_error_ref(&error),
                "RAG pipeline init failed"
            );
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
    let Some(snapshot) = managed_account_snapshot(paths)? else {
        return Ok(None);
    };
    Ok(Some(rag_scope_for_account(&snapshot.account)?))
}

struct ManagedAccountSnapshot {
    account: cue_core::AccountConfig,
    owner_account_id: String,
}

fn managed_account_snapshot(paths: &AppPaths) -> anyhow::Result<Option<ManagedAccountSnapshot>> {
    let Some(account) = load_account(paths)? else {
        return Ok(None);
    };
    if account.provider.trim() != "bluey" {
        return Ok(None);
    }
    let Some(owner_account_id) = account.owner_account_id().map(ToString::to_string) else {
        return Ok(None);
    };
    if crate::app::account_deletion_is_pending_for(paths, &owner_account_id)? {
        return Ok(None);
    }
    Ok(Some(ManagedAccountSnapshot {
        account,
        owner_account_id,
    }))
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
    let Some(snapshot) = managed_account_snapshot(paths)? else {
        return Ok(None);
    };
    if !load_settings(paths)?.cloud_sync_allowed_for_account(Some(&snapshot.owner_account_id)) {
        return Ok(None);
    }
    let access = snapshot
        .account
        .access_token
        .clone()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("managed RAG account credentials are unavailable"))?;
    let store = cue_cloud_client::tokens::MemoryStore::new();
    store.save(&cue_cloud_client::Tokens {
        access,
        refresh: snapshot.account.refresh_token.clone().unwrap_or_default(),
        email: snapshot.account.user_id.clone(),
    })?;
    let config = cue_cloud_client::client::ClientConfig {
        base_url: snapshot.account.api_url.clone(),
        ..Default::default()
    };
    let client = cue_cloud_client::CloudClient::new(config, Arc::new(store))?;
    info!("RAG embeddings configured through Bluey managed router");
    Ok(Some(Arc::new(ManagedBlueyEmbedder::new(
        client,
        paths.clone(),
        snapshot.owner_account_id,
        snapshot.account.credential_generation,
    ))))
}

fn cloud_embedding_consent_active(paths: &AppPaths) -> anyhow::Result<bool> {
    let owner_account_id =
        managed_account_snapshot(paths)?.map(|snapshot| snapshot.owner_account_id);
    Ok(load_settings(paths)?.cloud_sync_allowed_for_account(owner_account_id.as_deref()))
}

fn ensure_managed_embedding_context(
    paths: &AppPaths,
    expected_owner_account_id: &str,
    expected_credential_generation: u64,
) -> std::result::Result<(), EmbeddingError> {
    let snapshot = managed_account_snapshot(paths)
        .map_err(|_| {
            EmbeddingError::Request(
                "managed embedding is disabled because the account profile is unavailable"
                    .to_string(),
            )
        })?
        .ok_or_else(|| {
            EmbeddingError::Request(
                "managed embedding is disabled because the signed-in account changed".to_string(),
            )
        })?;
    if snapshot.owner_account_id != expected_owner_account_id
        || snapshot.account.credential_generation != expected_credential_generation
    {
        return Err(EmbeddingError::Request(
            "managed embedding is disabled because the signed-in account changed".to_string(),
        ));
    }
    let settings = load_settings(paths).map_err(|_| {
        EmbeddingError::Request(
            "managed embedding is disabled because persisted cloud consent is unavailable"
                .to_string(),
        )
    })?;
    if !settings.cloud_sync_allowed_for_account(Some(expected_owner_account_id)) {
        return Err(EmbeddingError::Request(
            concat!(
                "managed embedding is disabled until cloud sync and cloud consent are enabled ",
                "for this account"
            )
            .to_string(),
        ));
    }
    Ok(())
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

    #[test]
    fn rag_log_reference_is_stable_and_does_not_expose_session_id() {
        let session_id = "private-session-id-550e8400-e29b-41d4-a716-446655440000";
        let first = rag_session_ref(session_id);
        assert_eq!(first, rag_session_ref(session_id));
        assert_ne!(first, rag_session_ref("another-session"));
        assert!(!first.contains(session_id));
        assert_eq!(first.len(), 16);
    }
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
        let account_id = load_account(paths)
            .unwrap()
            .and_then(|account| account.owner_account_id().map(ToString::to_string))
            .unwrap_or_else(|| "test-account".to_string());
        save_settings(
            paths,
            &CueSettings {
                cloud_sync_enabled: enabled,
                cloud_sync_consent_granted: consent_granted,
                cloud_sync_consent_account_id: Some(account_id),
                ..CueSettings::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn query_filter_hides_tombstoned_vectors_before_physical_delete_finishes() {
        let root =
            std::env::temp_dir().join(format!("bluey-rag-query-fence-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let queue = RagIndexQueue::open(root.join("queue.db")).unwrap();
        let scope = RagScope::new("account-a", Some("workspace-a")).unwrap();
        queue
            .tombstone_session(&scope, "deleted-session", 100)
            .unwrap();
        let mut hits = vec![
            cue_rag::RagHit {
                session_id: "deleted-session".to_string(),
                chunk_text: "must not be returned".to_string(),
                score: 1.0,
            },
            cue_rag::RagHit {
                session_id: "live-session".to_string(),
                chunk_text: "safe result".to_string(),
                score: 0.5,
            },
        ];

        remove_tombstoned_hits(&queue, &scope, &mut hits);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "live-session");
        assert_eq!(
            queue
                .claim_next_vector_delete("worker", 101, Duration::from_secs(30))
                .unwrap()
                .unwrap()
                .session_id,
            "deleted-session"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rebuild_failure_reports_staging_cleanup_failure_instead_of_ignoring_it() {
        let error = combine_rebuild_and_cleanup_errors(
            anyhow::anyhow!("embedding failed"),
            Err(anyhow::anyhow!("vector delete failed")),
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("embedding failed"));
        assert!(message.contains("vector delete failed"));
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
    fn retained_managed_embedder_rejects_account_and_credential_changes() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            let mut account_a = linked_account("account-a", "a@example.com", "workspace-a");
            account_a.api_url = "http://127.0.0.1:9".to_string();
            save_account(&paths, &account_a).unwrap();
            save_cloud_consent(&paths, true, true);
            let embedder = managed_embedder(&paths).unwrap().expect("managed embedder");

            account_a.access_token = Some("rotated-access-a".to_string());
            save_account(&paths, &account_a).unwrap();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let error = runtime
                .block_on(embedder.embed("must not use a stale credential snapshot"))
                .expect_err("credential rotation must invalidate the retained provider");
            assert!(error.to_string().contains("account changed"));

            let fresh = managed_embedder(&paths).unwrap().expect("fresh embedder");
            save_account(
                &paths,
                &linked_account("account-b", "b@example.com", "workspace-b"),
            )
            .unwrap();
            let error = runtime
                .block_on(fresh.embed("must not cross account boundaries"))
                .expect_err("account switch must invalidate the retained provider");
            assert!(error.to_string().contains("account changed"));
        });
    }

    #[test]
    fn pending_account_deletion_disables_new_and_retained_managed_embeddings() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            let account = linked_account("account-a", "a@example.com", "workspace-a");
            save_account(&paths, &account).unwrap();
            save_cloud_consent(&paths, true, true);
            let retained = managed_embedder(&paths).unwrap().expect("managed embedder");

            std::fs::create_dir_all(&paths.data_dir).unwrap();
            std::fs::write(
                paths.data_dir.join("pending-deleted-account-purge.json"),
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": 3,
                    "owner_account_id": "account-a",
                    "operation_id": "550e8400-e29b-41d4-a716-446655440901",
                    "recovery_token": "550e8400-e29b-41d4-a716-446655440902",
                    "requested_at_ms": 1,
                    "state": "prepared"
                }))
                .unwrap(),
            )
            .unwrap();

            assert!(managed_embedder(&paths).unwrap().is_none());
            assert!(managed_account_scope(&paths).unwrap().is_none());
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let error = runtime
                .block_on(retained.embed("must remain local during account deletion"))
                .expect_err("pending deletion must invalidate a retained embedder");
            assert!(error.to_string().contains("account changed"));
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

    #[tokio::test]
    async fn account_purge_removes_orphan_queue_and_vector_rows_across_workspaces() {
        let paths = test_paths();
        paths.ensure().unwrap();
        let meeting_store = MeetingStore::new(&paths).unwrap();
        let coordinator = RagIndexCoordinator::from_paths(&paths, meeting_store).unwrap();
        let owner_a_one = RagScope::new("account-a", Some("workspace-one")).unwrap();
        let owner_a_two = RagScope::new("account-a", Some("workspace-two")).unwrap();
        let owner_b = RagScope::new("account-b", Some("workspace-one")).unwrap();
        let revision_a = "a".repeat(64);
        let revision_b = "b".repeat(64);
        coordinator
            .queue
            .enqueue(&owner_a_one, "known-session", &revision_a, 100)
            .unwrap();
        coordinator
            .queue
            .enqueue(&owner_a_two, "orphan-session", &revision_b, 100)
            .unwrap();
        coordinator
            .queue
            .enqueue(&owner_b, "other-session", &revision_a, 100)
            .unwrap();

        let vector_path = paths.data_dir.join("rag_vectors.db");
        let vectors = cue_rag::VectorStore::open(&vector_path, 3).unwrap();
        let chunk = cue_rag::Chunk {
            text: "orphan private memory".to_string(),
            start_char: 0,
            end_char: 21,
        };
        vectors
            .index(&owner_a_one, "known-session", &chunk, &[1.0, 0.0, 0.0])
            .unwrap();
        vectors
            .index(&owner_a_two, "orphan-session", &chunk, &[0.0, 1.0, 0.0])
            .unwrap();
        vectors
            .index(&owner_b, "other-session", &chunk, &[0.0, 0.0, 1.0])
            .unwrap();
        drop(vectors);

        assert!(
            coordinator
                .purge_account_for_owner("account-a")
                .await
                .unwrap()
                >= 4
        );
        assert!(coordinator
            .queue
            .account_is_tombstoned("account-a")
            .unwrap());
        assert_eq!(
            coordinator
                .queue
                .account_metadata_row_count("account-a")
                .unwrap(),
            0
        );
        assert_eq!(
            cue_rag::VectorStore::account_chunk_count_at_path(&vector_path, "account-a").unwrap(),
            0
        );
        assert_eq!(
            cue_rag::VectorStore::account_chunk_count_at_path(&vector_path, "account-b").unwrap(),
            1
        );
        assert!(!coordinator
            .queue
            .enqueue(&owner_a_one, "late-session", &revision_a, 101)
            .unwrap());

        let _ = std::fs::remove_dir_all(&paths.data_dir);
        let _ = std::fs::remove_dir_all(&paths.config_dir);
        let _ = std::fs::remove_dir_all(&paths.runtime_dir);
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
            assert!(coordinator.pipeline().is_none());

            save_cloud_consent(&paths, true, true);
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
