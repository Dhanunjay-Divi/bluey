//! Two-stage question detection, stage 2: the ONNX classifier (feature
//! `local-memory`; PLAN-CONTEXT-WARMUP SET 1).
//!
//! Stage 1 is the lexical [`cue_core::is_question_shaped`] check — fast and
//! precise, but it misses disfluent/declarative questions real meeting speech
//! is full of ("so um do we need the flag or not", "i wonder if that's thread
//! safe"). Stage 2 runs `shahrukhx01/question-vs-statement-classifier` (BERT
//! mini, int8, ~11MB) via `ort` on stage 1's REJECTS only, so the common path
//! stays regex-cheap. Measured on labeled meeting lines: wh-questions 92%,
//! overall recall regex 41% → two-stage 63%+.
//!
//! No hosted ONNX export of this model exists, so there is NO network
//! download here (unlike the bge embedder): `scripts/export-qdetect-onnx.sh`
//! exports it once (with an int8-vs-pytorch parity gate) and the AirDrop
//! packager ships it next to the binary. Model files absent → `load` fails
//! cleanly and detection stays regex-only — never fatal.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use ort::session::Session;
use tokenizers::Tokenizer;
use tracing::info;

const MODEL_FILE: &str = "model_int8.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// Transcript lines are short; cap way below BERT's 512 positions anyway.
const MAX_TOKENS: usize = 128;

/// The question-vs-statement classifier. Cheap to clone (Arc internals);
/// `Session::run` needs `&mut`, so the session sits behind a std Mutex —
/// passes are single-line and take ~1ms, and callers run on `spawn_blocking`.
#[derive(Clone)]
pub struct QuestionClassifier {
    session: Arc<Mutex<Session>>,
    tokenizer: Arc<Tokenizer>,
    /// Input names the model actually declares; we feed exactly those.
    input_names: Arc<Vec<String>>,
}

impl QuestionClassifier {
    /// Resolve the model dir and load. `None` (with a debug reason) when the
    /// model is not present — the caller keeps regex-only detection.
    pub fn load_for(paths: &AppPaths) -> Result<Self> {
        let dir = resolve_model_dir(paths)
            .context("qdetect model not found (BLUEY_QDETECT_MODEL_DIR / data dir / bundle)")?;
        let this = Self::load(&dir.join(MODEL_FILE), &dir.join(TOKENIZER_FILE))?;
        info!(dir = %dir.display(), "question classifier ready (two-stage detection on)");
        Ok(this)
    }

    /// Load from explicit local files. Fails cleanly on missing/corrupt files.
    pub fn load(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .and_then(|mut b| b.commit_from_file(model_path))
            .map_err(|e| anyhow::anyhow!("onnx session: {e}"))?;
        let input_names = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect::<Vec<_>>();
        let tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow::anyhow!("tokenizer: {e}"))?;
        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            input_names: Arc::new(input_names),
        })
    }

    /// Classify one line synchronously (call from a blocking context).
    /// `true` = question (the model's LABEL_1).
    pub fn classify_sync(&self, text: &str) -> Result<bool> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?;
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
            return Ok(false);
        }
        let type_ids = vec![0i64; len];

        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("classifier session poisoned"))?;
        let mut inputs: Vec<(
            std::borrow::Cow<'_, str>,
            ort::session::SessionInputValue<'_>,
        )> = Vec::new();
        for name in self.input_names.iter() {
            let data = match name.as_str() {
                "input_ids" => ids.clone(),
                "attention_mask" => mask.clone(),
                "token_type_ids" => type_ids.clone(),
                other => anyhow::bail!("unexpected model input: {other}"),
            };
            let tensor = ort::value::Tensor::from_array(([1usize, len], data))
                .map_err(|e| anyhow::anyhow!("tensor: {e}"))?;
            inputs.push((name.as_str().into(), tensor.into()));
        }
        let outputs = session
            .run(inputs)
            .map_err(|e| anyhow::anyhow!("onnx run: {e}"))?;
        // Single output: logits [1, 2] — LABEL_0 statement, LABEL_1 question.
        let (_, logits) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("extract: {e}"))?;
        if logits.len() < 2 {
            anyhow::bail!("logits too small: {}", logits.len());
        }
        Ok(logits[1] > logits[0])
    }

    /// Classify on a blocking thread (the async detection path calls this).
    pub async fn classify(&self, text: &str) -> Result<bool> {
        let this = self.clone();
        let text = text.to_string();
        tokio::task::spawn_blocking(move || this.classify_sync(&text))
            .await
            .context("classifier join")?
    }
}

/// Model-dir resolution, first hit wins:
/// 1. `BLUEY_QDETECT_MODEL_DIR` (tests / dev override)
/// 2. `<data_dir>/models/qdetect-en` (user-placed)
/// 3. `<exe_dir>/models/qdetect-en` (the AirDrop bundle layout)
fn resolve_model_dir(paths: &AppPaths) -> Option<PathBuf> {
    let has_model = |d: &Path| d.join(MODEL_FILE).is_file() && d.join(TOKENIZER_FILE).is_file();
    if let Ok(dir) = std::env::var("BLUEY_QDETECT_MODEL_DIR") {
        let dir = PathBuf::from(dir.trim());
        if has_model(&dir) {
            return Some(dir);
        }
    }
    let data = paths.data_dir.join("models").join("qdetect-en");
    if has_model(&data) {
        return Some(data);
    }
    // Canonicalize: the installer launches the daemon through a symlink
    // (~/.local/bin/bluey-daemon → <install>/bin/bluey-daemon); the model dir
    // sits next to the REAL binary.
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let bundled = exe.parent()?.join("models").join("qdetect-en");
    has_model(&bundled).then_some(bundled)
}
