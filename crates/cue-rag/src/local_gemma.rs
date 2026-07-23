//! Local ONNX embedder — `EmbeddingGemma-300M` via `ort` (feature `local-embed`).
//!
//! Google's 2026 on-device embedding model. Unlike the bge path, the ONNX graph
//! emits a pooled+normalized `sentence_embedding` output directly (shape
//! `[1, 768]`), so this module does NOT pool manually — it reads that output and
//! (optionally) applies Matryoshka (MRL) truncation to a smaller dimension, then
//! re-normalizes (the MRL convention).
//!
//! Signature verified live (`tests/gemma_probe.rs`):
//! - inputs:  `input_ids`, `attention_mask`
//! - outputs: `last_hidden_state [1,seq,768]`, `sentence_embedding [1,768]`
//!
//! Prompt convention (EmbeddingGemma model card): the input string is prefixed
//! per task. Retrieval query → `task: search result | query: {text}`; document/
//! passage → `title: none | text: {text}`.
//!
//! Model files (`model_quantized.onnx` + its `.onnx_data` sidecar +
//! `tokenizer.json`, ~300MB int8) are fetched by the daemon on first run; this
//! module only loads from local paths.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ort::session::Session;
use tokenizers::Tokenizer;

use crate::embedder::{EmbeddingError, EmbeddingProvider};

/// EmbeddingGemma retrieval-query prompt (model card default task).
const QUERY_PREFIX: &str = "task: search result | query: ";
/// EmbeddingGemma document/passage prompt.
const DOC_PREFIX: &str = "title: none | text: ";

/// Native output dimension of EmbeddingGemma.
const FULL_DIM: usize = 768;
/// Gemma3 context is long, but our inputs (facts/prose turns) are short; cap to
/// keep inference bounded and match the other embedder's discipline.
const MAX_TOKENS: usize = 512;

/// Local EmbeddingGemma embedder. Cheap to clone (Arc internals). The output
/// dimension is `out_dim` — `FULL_DIM` (768) by default, or an MRL-truncated
/// size (256/128) chosen at load. Truncation keeps the FIRST `out_dim` entries
/// then re-normalizes (the Matryoshka convention).
#[derive(Clone)]
pub struct LocalGemmaEmbedder {
    session: Arc<Mutex<Session>>,
    tokenizer: Arc<Tokenizer>,
    input_names: Arc<Vec<String>>,
    /// Index of the `sentence_embedding` output among the model's outputs.
    sentence_out_idx: usize,
    out_dim: usize,
}

impl LocalGemmaEmbedder {
    /// Load the model + tokenizer. `out_dim` selects the MRL output size — pass
    /// `None` for the full 768, or `Some(256)` / `Some(128)` for a truncated
    /// (smaller, faster-to-compare, less-RAM-in-store) embedding. Values above
    /// 768 are clamped to 768.
    pub fn load(
        model_path: &Path,
        tokenizer_path: &Path,
        out_dim: Option<usize>,
    ) -> Result<Self, EmbeddingError> {
        let session = Session::builder()
            .and_then(|mut b| b.commit_from_file(model_path))
            .map_err(|e| EmbeddingError::Request(format!("onnx session: {e}")))?;
        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        // Find the pooled sentence-embedding output by name (order is not
        // guaranteed across exports); fall back to the last output.
        let outputs: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let sentence_out_idx = outputs
            .iter()
            .position(|n| n == "sentence_embedding")
            .unwrap_or(outputs.len().saturating_sub(1));
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| EmbeddingError::Request(format!("tokenizer: {e}")))?;
        let out_dim = out_dim.unwrap_or(FULL_DIM).clamp(1, FULL_DIM);
        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            input_names: Arc::new(input_names),
            sentence_out_idx,
            out_dim,
        })
    }

    /// Embed one text synchronously. `is_query` applies the query prompt;
    /// otherwise the document prompt (EmbeddingGemma is asymmetric).
    pub fn embed_sync(&self, text: &str, is_query: bool) -> Result<Vec<f32>, EmbeddingError> {
        let prefix = if is_query { QUERY_PREFIX } else { DOC_PREFIX };
        let input = format!("{prefix}{text}");
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
        // Pooled sentence embedding [1, 768] — the graph already pooled it.
        let (_, data) = outputs[self.sentence_out_idx]
            .try_extract_tensor::<f32>()
            .map_err(|e| EmbeddingError::InvalidResponse(format!("extract: {e}")))?;
        if data.len() < self.out_dim {
            return Err(EmbeddingError::InvalidResponse(format!(
                "output {} smaller than requested dim {}",
                data.len(),
                self.out_dim
            )));
        }
        // MRL truncation: keep the first `out_dim` entries, then L2-normalize
        // (Gemma may already normalize at 768, but truncation breaks the norm,
        // so we always renormalize to be safe and dim-consistent).
        let mut v = data[..self.out_dim].to_vec();
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }

    /// Embed a retrieval QUERY on a blocking thread.
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed_sync(&text, true))
            .await
            .map_err(|e| EmbeddingError::Request(format!("join: {e}")))?
    }
}

#[async_trait]
impl EmbeddingProvider for LocalGemmaEmbedder {
    fn name(&self) -> &'static str {
        "local-embeddinggemma-300m"
    }

    fn dim(&self) -> usize {
        self.out_dim
    }

    /// Passage embedding (document prompt) — what gets INDEXED.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.embed_sync(&text, false))
            .await
            .map_err(|e| EmbeddingError::Request(format!("join: {e}")))?
    }

    /// Query embedding (Gemma query prompt applied via the inherent method).
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        LocalGemmaEmbedder::embed_query(self, text).await
    }

    /// Gemma's cosine scores are compressed to a LOWER range than bge's — its
    /// correct hits on real meeting prose scored 0.226–0.538 (bge's 0.45 floor
    /// would discard nearly all of them). Measured in `tests/embedder_bakeoff.rs`;
    /// 0.20 keeps correct hits while the max wrong-top was 0.250 (thin but real
    /// separation, so the pipeline's other signals still resolve ties).
    fn relevance_floor(&self) -> f32 {
        0.20
    }

    /// Direct sync passage embed — no runtime hop (callers are already blocking).
    fn embed_passage_blocking(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        self.embed_sync(text, false)
    }
}
