//! Local ONNX embedder — `bge-small-en-v1.5` via `ort` (feature `local-embed`).
//!
//! Keyless, on-device embeddings for semantic memory (PLAN-CONTEXT-WARMUP
//! Appendix A/E): 384-dim, CLS-pooled + L2-normalized, with the bge retrieval
//! query prefix. Measured on extracted meeting facts: 90% top-1 / 100% top-3 —
//! the store must hold EXTRACTED FACTS, not raw transcript (the measured rule).
//!
//! The model files (`model_int8.onnx` + `tokenizer.json`, ~35MB total) are
//! fetched by the daemon on first run (same pattern as the STT model); this
//! module only loads from local paths. Inference runs on `spawn_blocking` so
//! the async `EmbeddingProvider::embed` never blocks a runtime worker.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ort::session::Session;
use tokenizers::Tokenizer;

use crate::embedder::{EmbeddingError, EmbeddingProvider};

/// bge-v1.5 retrieval convention: queries get this prefix, passages do not.
const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// Hard cap on input tokens (BERT position limit is 512).
const MAX_TOKENS: usize = 512;

/// Local bge-small embedder. Cheap to clone (Arc internals); `Session::run`
/// needs `&mut`, so the session sits behind a std Mutex — passes are short
/// (a few ms) and callers already run on `spawn_blocking`.
#[derive(Clone)]
pub struct LocalBgeEmbedder {
    session: Arc<Mutex<Session>>,
    tokenizer: Arc<Tokenizer>,
    /// Input names the model actually declares (some exports omit
    /// `token_type_ids`); we feed exactly what is declared.
    input_names: Arc<Vec<String>>,
}

impl LocalBgeEmbedder {
    pub const DIM: usize = 384;

    /// Load the model + tokenizer from local files. Fails cleanly when the
    /// files are missing/corrupt — the caller treats that as "memory off".
    pub fn load(model_path: &Path, tokenizer_path: &Path) -> Result<Self, EmbeddingError> {
        let session = Session::builder()
            .and_then(|mut b| b.commit_from_file(model_path))
            .map_err(|e| EmbeddingError::Request(format!("onnx session: {e}")))?;
        let input_names = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect::<Vec<_>>();
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| EmbeddingError::Request(format!("tokenizer: {e}")))?;
        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            input_names: Arc::new(input_names),
        })
    }

    /// Embed one text synchronously (call from a blocking context).
    /// `is_query` applies the bge retrieval prefix.
    pub fn embed_sync(&self, text: &str, is_query: bool) -> Result<Vec<f32>, EmbeddingError> {
        let input = if is_query {
            format!("{QUERY_PREFIX}{text}")
        } else {
            text.to_string()
        };
        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| EmbeddingError::Request(format!("tokenize: {e}")))?;
        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
        let mut mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&v| v as i64)
            .collect();
        ids.truncate(MAX_TOKENS);
        mask.truncate(MAX_TOKENS);
        let len = ids.len();
        if len == 0 {
            return Err(EmbeddingError::InvalidResponse("empty encoding".into()));
        }
        let type_ids = vec![0i64; len];

        let mut session = self
            .session
            .lock()
            .map_err(|_| EmbeddingError::Request("embedder session poisoned".into()))?;
        let mut inputs: Vec<(
            std::borrow::Cow<'_, str>,
            ort::session::SessionInputValue<'_>,
        )> = Vec::new();
        for name in self.input_names.iter() {
            let data = match name.as_str() {
                "input_ids" => ids.clone(),
                "attention_mask" => mask.clone(),
                "token_type_ids" => type_ids.clone(),
                other => {
                    return Err(EmbeddingError::InvalidResponse(format!(
                        "unexpected model input: {other}"
                    )))
                }
            };
            let tensor = ort::value::Tensor::from_array(([1usize, len], data))
                .map_err(|e| EmbeddingError::Request(format!("tensor: {e}")))?;
            inputs.push((name.as_str().into(), tensor.into()));
        }
        let outputs = session
            .run(inputs)
            .map_err(|e| EmbeddingError::Request(format!("onnx run: {e}")))?;
        // First declared output = last_hidden_state [1, seq, 384].
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EmbeddingError::InvalidResponse(format!("extract: {e}")))?;
        if data.len() < Self::DIM {
            return Err(EmbeddingError::InvalidResponse(format!(
                "output too small: {}",
                data.len()
            )));
        }
        // CLS pooling (bge convention: first token) + L2 normalize.
        let mut cls = data[..Self::DIM].to_vec();
        let norm = cls.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut cls {
                *v /= norm;
            }
        }
        Ok(cls)
    }

    /// Embed a retrieval QUERY on a blocking thread (prefix applied).
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed_sync(&text, true))
            .await
            .map_err(|e| EmbeddingError::Request(format!("join: {e}")))?
    }
}

#[async_trait]
impl EmbeddingProvider for LocalBgeEmbedder {
    fn name(&self) -> &'static str {
        "local-bge-small"
    }

    fn dim(&self) -> usize {
        Self::DIM
    }

    /// Passage embedding (no query prefix) — what gets INDEXED.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed_sync(&text, false))
            .await
            .map_err(|e| EmbeddingError::Request(format!("join: {e}")))?
    }

    /// Query embedding (bge/arctic prefix applied via the inherent method).
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        LocalBgeEmbedder::embed_query(self, text).await
    }

    /// bge/arctic score floor (measured ~0.45 on real meeting prose).
    fn relevance_floor(&self) -> f32 {
        0.45
    }

    /// Direct sync passage embed — no runtime hop (callers are already blocking).
    fn embed_passage_blocking(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        self.embed_sync(text, false)
    }
}
