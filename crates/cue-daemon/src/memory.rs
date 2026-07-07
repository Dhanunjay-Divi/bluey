//! Cross-meeting facts memory — daemon orchestration (feature `local-memory`).
//!
//! The long-term tier of the two-tier memory model (PLAN-CONTEXT-WARMUP
//! Appendix A/E): quote-verified ledger facts are embedded with the LOCAL
//! bge-small ONNX embedder (keyless, on-device — never OpenAI) and stored in
//! `facts_memory.db`, then recalled semantically across ALL past meetings on
//! the answer path. The store holds EXTRACTED FACTS, never raw transcript
//! (the measured rule: facts ≈100% top-3 recall; raw transcript confidently
//! mismatches).
//!
//! Model files (`model_int8.onnx` + `tokenizer.json`, ~35MB) download once on
//! first run — same install-and-it-just-works pattern as the STT model — into
//! `<data_dir>/models/bge-small-en/` (override: `BLUEY_EMBED_MODEL_DIR`;
//! mirror: `BLUEY_EMBED_MODEL_URL_BASE`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use cue_rag::{AddOutcome, EmbeddingProvider, FactHit, FactsStore, LocalBgeEmbedder};
use tracing::{debug, info};

/// Xenova's ONNX export of BAAI/bge-small-en-v1.5 (verified public, int8 +
/// tokenizer.json). int8 keeps the download small (~34MB) at negligible
/// retrieval cost for extracted-fact inputs.
const DEFAULT_MODEL_URL_BASE: &str = "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main";

const MODEL_FILE: &str = "model_int8.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// The assembled long-term memory: local embedder + facts store.
pub struct FactsMemory {
    embedder: LocalBgeEmbedder,
    store: FactsStore,
}

impl FactsMemory {
    /// Ensure model files (downloading on first run), load the embedder, open
    /// the store. Any failure means "memory off" — never fatal to the daemon.
    pub async fn ensure(paths: &AppPaths) -> Result<Self> {
        let model_dir = resolve_model_dir(paths);
        tokio::fs::create_dir_all(&model_dir)
            .await
            .with_context(|| format!("create {}", model_dir.display()))?;
        let base = std::env::var("BLUEY_EMBED_MODEL_URL_BASE")
            .ok()
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| DEFAULT_MODEL_URL_BASE.to_string());

        let model_path = model_dir.join(MODEL_FILE);
        if !model_path.is_file() {
            info!("embedding model not found; downloading on first run (~34MB, one-time)");
            download_file(&format!("{base}/onnx/{MODEL_FILE}"), &model_path).await?;
        }
        let tokenizer_path = model_dir.join(TOKENIZER_FILE);
        if !tokenizer_path.is_file() {
            download_file(&format!("{base}/{TOKENIZER_FILE}"), &tokenizer_path).await?;
        }

        // Session load is blocking CPU work — keep it off the async runtime.
        let embedder = {
            let (m, t) = (model_path.clone(), tokenizer_path.clone());
            tokio::task::spawn_blocking(move || LocalBgeEmbedder::load(&m, &t))
                .await
                .context("embedder load task")?
                .map_err(|e| anyhow::anyhow!("load local embedder: {e}"))?
        };
        let store = FactsStore::open(
            &paths.data_dir.join("facts_memory.db"),
            LocalBgeEmbedder::DIM,
        )?;
        info!(
            facts = store.current_len().unwrap_or(0),
            "cross-meeting facts memory ready (local bge-small, keyless)"
        );
        Ok(Self { embedder, store })
    }

    /// Embed + index one extracted fact (ADD / NOOP / SUPERSEDE — see
    /// [`cue_rag::facts`]). Embedding runs on a blocking thread.
    pub async fn index_fact(&self, meeting_id: &str, text: &str) -> Result<AddOutcome> {
        let embedding = self
            .embedder
            .embed(text)
            .await
            .map_err(|e| anyhow::anyhow!("embed fact: {e}"))?;
        self.store.add_fact(meeting_id, text, &embedding)
    }

    /// Cross-meeting semantic recall for a question, excluding the active
    /// meeting (its ledger is already pinned in context).
    pub async fn search(
        &self,
        question: &str,
        k: usize,
        exclude_meeting: Option<&str>,
    ) -> Result<Vec<FactHit>> {
        let embedding = self
            .embedder
            .embed_query(question)
            .await
            .map_err(|e| anyhow::anyhow!("embed query: {e}"))?;
        self.store.query(&embedding, k, exclude_meeting)
    }
}

/// Resolve the embedding-model dir: env override, else
/// `<data_dir>/models/bge-small-en`.
fn resolve_model_dir(paths: &AppPaths) -> PathBuf {
    if let Ok(dir) = std::env::var("BLUEY_EMBED_MODEL_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    paths.data_dir.join("models").join("bge-small-en")
}

/// Stream one file to `dest` (`.part` + rename so an interrupted download never
/// leaves a half-written model file). Mirrors the STT model_setup pattern.
async fn download_file(url: &str, dest: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    debug!(%url, "downloading embedding model file");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .context("build embed-model HTTP client")?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("download {url} returned HTTP {}", resp.status());
    }
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .with_context(|| format!("create {}", tmp.display()))?;
    let mut written: u64 = 0;
    while let Some(chunk) = resp
        .chunk()
        .await
        .with_context(|| format!("stream error during {url}"))?
    {
        file.write_all(&chunk)
            .await
            .with_context(|| format!("write error to {}", tmp.display()))?;
        written += chunk.len() as u64;
    }
    file.flush().await.ok();
    drop(file);
    if written < 1_024 {
        let _ = tokio::fs::remove_file(&tmp).await;
        anyhow::bail!("download {url} produced only {written} bytes (likely an error page)");
    }
    tokio::fs::rename(&tmp, dest)
        .await
        .with_context(|| format!("finalize {}", dest.display()))?;
    info!(dest = %dest.display(), bytes = written, "embedding model file ready");
    Ok(())
}

/// Render cross-meeting hits as one bounded context block, oldest-last so the
/// most relevant (highest score) leads. Empty string when no hit clears the
/// relevance floor — semantic noise must not pollute the answer context.
pub fn render_hits(hits: &[FactHit], min_score: f32, max_chars: usize) -> String {
    let mut block = String::new();
    for hit in hits.iter().filter(|h| h.score >= min_score) {
        let line = format!("- {}\n", hit.text.trim());
        if block.len() + line.len() > max_chars {
            break;
        }
        block.push_str(&line);
    }
    block.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(text: &str, score: f32) -> FactHit {
        FactHit {
            text: text.to_string(),
            meeting_id: "m".to_string(),
            score,
            created_at_ms: 0,
        }
    }

    #[test]
    fn render_hits_filters_by_score_and_bounds_size() {
        let hits = vec![
            hit("shard by tenant id", 0.82),
            hit("checkout sla 200ms", 0.71),
            hit("irrelevant noise", 0.20),
        ];
        let block = render_hits(&hits, 0.5, 200);
        assert!(block.contains("shard by tenant"));
        assert!(block.contains("checkout sla"));
        assert!(!block.contains("noise"), "low-score hits must be dropped");

        let bounded = render_hits(&hits, 0.0, 25);
        assert!(bounded.len() <= 25);
    }
}
