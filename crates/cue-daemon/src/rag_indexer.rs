use std::sync::{Arc, RwLock};

use anyhow::Result;
use async_trait::async_trait;
use cue_cloud_client::TokenStore;
use cue_core::{app_paths::AppPaths, load_account, new_request_id, ContextArtifact, MeetingRecord};
use cue_rag::{EmbeddingError, EmbeddingProvider, RagScope};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::storage::MeetingStore;

const MANAGED_EMBED_DIM: usize = cue_rag::embedder::OpenAiEmbedder::DIM;
const MAX_MANAGED_EMBED_INPUT_CHARS: usize = 8_192;
const LOCAL_RAG_ACCOUNT_ID: &str = "__bluey_local_account__";

struct ManagedBlueyEmbedder {
    client: cue_cloud_client::CloudClient,
}

impl ManagedBlueyEmbedder {
    fn new(client: cue_cloud_client::CloudClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl EmbeddingProvider for ManagedBlueyEmbedder {
    fn name(&self) -> &'static str {
        "bluey-managed"
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

        let response = match self
            .client
            .embed_batch(&cue_cloud_client::EmbedBatchRequest {
                request_id: new_request_id(),
                inputs: inputs.clone(),
                model: None,
            })
            .await
        {
            Ok(response) => response,
            Err(cue_cloud_client::Error::Server { status }) if status == 404 || status == 405 => {
                let mut vectors = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let response = self
                        .client
                        .embed(&cue_cloud_client::EmbedRequest {
                            request_id: new_request_id(),
                            input,
                            model: None,
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
    paths: AppPaths,
}

impl RagIndexCoordinator {
    pub(crate) fn from_paths(paths: &AppPaths) -> Self {
        Self {
            pipeline: Arc::new(RwLock::new(init_rag_pipeline(paths))),
            session_lock: Arc::new(Mutex::new(())),
            paths: paths.clone(),
        }
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
        if replacement_scope.is_some() {
            info!("RAG pipeline refreshed for current account scope");
        } else {
            info!("RAG pipeline cleared after authentication change");
        }
        true
    }

    pub(crate) fn index_transcript(&self, store: MeetingStore, session_id: String, text: String) {
        let Some(rag) = self.pipeline() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }

        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if !session_still_exists(&store, &session_id, rag.scope(), "RAG transcript index") {
                return;
            }
            if let Err(error) = rag.claim_legacy_session(&session_id).await {
                warn!(session_id = %session_id, error = %error, "failed to claim legacy RAG rows before transcript index");
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
        let Some(rag) = self.pipeline() else {
            return;
        };
        if artifacts.is_empty() {
            return;
        }

        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if !session_still_exists(&store, &session_id, rag.scope(), "RAG artifact index") {
                return;
            }
            if let Err(error) = rag.claim_legacy_session(&session_id).await {
                warn!(session_id = %session_id, error = %error, "failed to claim legacy RAG rows before artifact index");
                return;
            }
            for artifact in artifacts {
                rag.index_context_artifact(&session_id, &artifact).await;
            }
        });
    }

    pub(crate) fn reindex_meeting(&self, store: MeetingStore, meeting: MeetingRecord) {
        let Some(rag) = self.pipeline() else {
            return;
        };

        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            rebuild_meeting_rag_index(rag, store, session_lock, meeting, "session reindex").await
        });
    }

    pub(crate) fn delete_session(&self, session_id: String) {
        let Some(rag) = self.pipeline() else {
            return;
        };

        let session_lock = Arc::clone(&self.session_lock);
        tokio::spawn(async move {
            let _guard = session_lock.lock().await;
            if let Err(error) = rag.delete_session(&session_id).await {
                warn!(session_id = %session_id, error = %error, "failed to clear deleted session RAG index");
            }
        });
    }

    pub(crate) async fn query_current_and_global(
        &self,
        query_text: &str,
        current_limit: usize,
        current_session_id: &str,
        global_limit: usize,
    ) -> Result<(Vec<cue_rag::RagHit>, Vec<cue_rag::RagHit>)> {
        let Some(rag) = self.pipeline() else {
            return Ok((Vec::new(), Vec::new()));
        };
        let query_scope = rag.scope().clone();
        let results = rag
            .query_current_and_global(query_text, current_limit, current_session_id, global_limit)
            .await?;
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
}

fn session_still_exists(
    store: &MeetingStore,
    session_id: &str,
    scope: &RagScope,
    action: &'static str,
) -> bool {
    let Ok(session_uuid) = uuid::Uuid::parse_str(session_id) else {
        warn!(session_id = %session_id, action, "skipping RAG work for invalid session id");
        return false;
    };
    match store.load_by_id(session_uuid) {
        Ok(Some(meeting)) if meeting_belongs_to_rag_scope(&meeting, scope) => true,
        Ok(Some(_)) => {
            warn!(session_id = %session_id, action, "skipping RAG work for a session owned by another account");
            false
        }
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

fn meeting_belongs_to_rag_scope(meeting: &MeetingRecord, scope: &RagScope) -> bool {
    let owner_account_id = meeting
        .owner_account_id
        .as_deref()
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
    store: MeetingStore,
    session_lock: Arc<Mutex<()>>,
    meeting: MeetingRecord,
    reason: &'static str,
) {
    let _guard = session_lock.lock().await;
    let session_id = meeting.id.to_string();
    if !session_still_exists(&store, &session_id, rag.scope(), reason) {
        return;
    }
    if let Err(error) = rag.claim_legacy_session(&session_id).await {
        warn!(session_id = %session_id, reason, error = %error, "failed to claim legacy RAG rows before rebuild");
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

/// Initialize the RAG pipeline if a managed Bluey account is linked.
///
/// Production/customer installs use `/router/embed`, so provider API keys stay
/// on `bluey-server`. Direct OpenAI embedding remains an explicit development
/// fallback only.
fn init_rag_pipeline(paths: &AppPaths) -> Option<Arc<crate::db::rag::RagPipeline>> {
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
                dev_openai_embedder()?
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
    if let Some(scope) = managed_account_scope(paths)? {
        return Ok(Some(scope));
    }
    if dev_openai_api_key().is_some() {
        return Ok(Some(local_rag_scope()));
    }
    Ok(None)
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
    Ok(Some(Arc::new(ManagedBlueyEmbedder::new(client))))
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
    use cue_core::{save_account, AccountConfig};
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

    #[test]
    fn managed_embedder_requires_linked_account_token() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
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

            let embedder = managed_embedder(&paths).unwrap().expect("managed embedder");
            assert_eq!(embedder.name(), "bluey-managed");
            assert_eq!(embedder.dim(), MANAGED_EMBED_DIM);
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

    #[test]
    fn coordinator_refreshes_after_account_link() {
        with_plaintext_token_fallback(|| {
            let paths = test_paths();
            paths.ensure().unwrap();
            let coordinator = RagIndexCoordinator::from_paths(&paths);
            assert!(coordinator.pipeline().is_none());
            assert!(!coordinator.refresh_from_paths(&paths));

            let mut account = AccountConfig::local();
            account.provider = "bluey".to_string();
            account.api_url = "http://127.0.0.1:8787".to_string();
            account.user_id = "tester@bluey.sh".to_string();
            account.access_token = Some("desktop-access-token".to_string());
            account.refresh_token = Some("desktop-refresh-token".to_string());
            save_account(&paths, &account).unwrap();

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

            let coordinator = RagIndexCoordinator::from_paths(&paths);
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
}
