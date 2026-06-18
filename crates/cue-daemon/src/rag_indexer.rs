use std::sync::Arc;

use anyhow::Result;
use cue_core::{app_paths::AppPaths, ContextArtifact, MeetingRecord};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::storage::MeetingStore;

#[derive(Clone)]
pub(crate) struct RagIndexCoordinator {
    pipeline: Option<Arc<crate::db::rag::RagPipeline>>,
    session_lock: Arc<Mutex<()>>,
}

impl RagIndexCoordinator {
    pub(crate) fn from_paths(paths: &AppPaths) -> Self {
        Self {
            pipeline: init_rag_pipeline(paths),
            session_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn index_transcript(&self, store: MeetingStore, session_id: String, text: String) {
        let Some(rag) = self.pipeline.as_ref() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }

        let rag = Arc::clone(rag);
        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if !session_still_exists(&store, &session_id, "RAG transcript index") {
                return;
            }
            rag.index_transcript(&session_id, &text).await;
        });
    }

    pub(crate) fn index_context_artifacts(
        &self,
        store: MeetingStore,
        session_id: String,
        artifacts: Vec<ContextArtifact>,
    ) {
        let Some(rag) = self.pipeline.as_ref() else {
            return;
        };
        if artifacts.is_empty() {
            return;
        }

        let rag = Arc::clone(rag);
        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if !session_still_exists(&store, &session_id, "RAG artifact index") {
                return;
            }
            for artifact in artifacts {
                rag.index_context_artifact(&session_id, &artifact).await;
            }
        });
    }

    pub(crate) fn reindex_meeting(&self, store: MeetingStore, meeting: MeetingRecord) {
        let Some(rag) = self.pipeline.as_ref() else {
            return;
        };

        let rag = Arc::clone(rag);
        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            rebuild_meeting_rag_index(rag, store, session_lock, meeting, "session reindex").await
        });
    }

    pub(crate) fn delete_session(&self, session_id: String) {
        let Some(rag) = self.pipeline.as_ref() else {
            return;
        };

        let rag = Arc::clone(rag);
        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if let Err(error) = rag.delete_session(&session_id).await {
                warn!(session_id = %session_id, error = %error, "failed to clear deleted session RAG index");
            }
        });
    }

    pub(crate) async fn query(
        &self,
        query_text: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<cue_rag::RagHit>> {
        let Some(rag) = self.pipeline.as_ref() else {
            return Ok(Vec::new());
        };
        rag.query(query_text, limit, session_id).await
    }
}

fn session_still_exists(store: &MeetingStore, session_id: &str, action: &'static str) -> bool {
    let Ok(session_uuid) = uuid::Uuid::parse_str(session_id) else {
        warn!(session_id = %session_id, action, "skipping RAG work for invalid session id");
        return false;
    };
    match store.load_by_id(session_uuid) {
        Ok(Some(_)) => true,
        Ok(None) => {
            debug!(session_id = %session_id, action, "skipping RAG work for deleted session");
            false
        }
        Err(error) => {
            warn!(session_id = %session_id, action, error = %error, "skipping RAG work after session lookup failed");
            false
        }
    }
}

async fn rebuild_meeting_rag_index(
    rag: Arc<crate::db::rag::RagPipeline>,
    store: MeetingStore,
    session_lock: Arc<Mutex<()>>,
    meeting: MeetingRecord,
    reason: &'static str,
) {
    let _guard = session_lock.lock().await;
    let session_id = meeting.id.to_string();
    if !session_still_exists(&store, &session_id, reason) {
        return;
    }
    if let Err(error) = rag.delete_session(&session_id).await {
        warn!(session_id = %session_id, reason, error = %error, "failed to clear RAG session before rebuild");
        return;
    }

    if let Some(summary) = meeting
        .summary
        .as_ref()
        .filter(|summary| !summary.trim().is_empty())
    {
        rag.index_transcript(
            &session_id,
            &format!("Compacted session summary:\n{}", summary.trim()),
        )
        .await;
    }

    for turn in &meeting.conversation {
        let question = turn.question.trim();
        let answer = turn.answer.trim();
        if !question.is_empty() || !answer.is_empty() {
            rag.index_transcript(
                &session_id,
                &format!("Prior Bluey answer\nQuestion: {question}\nAnswer: {answer}"),
            )
            .await;
        }
    }

    for segment in &meeting.transcript {
        if segment.is_final && !segment.text.trim().is_empty() {
            rag.index_transcript(&session_id, &segment.text).await;
        }
    }
    for artifact in &meeting.context {
        rag.index_context_artifact(&session_id, artifact).await;
    }
}

/// Initialize the RAG pipeline if an OpenAI API key is available.
/// Returns None (with a log) if no key is configured — RAG is optional.
fn init_rag_pipeline(paths: &AppPaths) -> Option<Arc<crate::db::rag::RagPipeline>> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| {
            env_truthy("BLUEY_DEV_BYOK")
                .then(|| crate::secrets::load_api_key("openai").ok().flatten())
                .flatten()
        });
    let Some(api_key) = api_key else {
        info!("RAG pipeline disabled: no OpenAI API key configured");
        return None;
    };
    let embedder = Arc::new(cue_rag::embedder::OpenAiEmbedder::new(api_key));
    let store_path = paths.data_dir.join("rag_vectors.db");
    match crate::db::rag::RagPipeline::new(store_path, embedder) {
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

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
