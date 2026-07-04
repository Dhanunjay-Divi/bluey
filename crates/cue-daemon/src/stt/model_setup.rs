//! First-run model provisioning for the on-device Parakeet STT backend.
//!
//! The product promise is "install it and it just works" — so the daemon
//! resolves the model location and, if the files aren't present yet, downloads
//! them ONCE into the app data dir and caches them. Nothing is hardcoded: the
//! location defaults to `<data_dir>/models/parakeet-en` and is overridable via
//! `BLUEY_PARAKEET_MODEL_DIR` (dev). After the one-time fetch everything runs
//! 100% on-device — no per-meeting network, no data leaving.
//!
//! For enterprise / air-gapped installs, ship the model inside the installer and
//! point `BLUEY_PARAKEET_MODEL_DIR` at it: the files-already-present check makes
//! the download a no-op (offline-bundle path). See docs/DECISION-VOICE-STT-STACK.md.
//!
//! Gated behind the `parakeet-stt` feature (the only caller is the Parakeet
//! provider path).
#![cfg(feature = "parakeet-stt")]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use tracing::{info, warn};

use super::parakeet::ParakeetPaths;

/// The three files a Nemotron English model dir must contain (verified against
/// `parakeet-rs` `NemotronModel::from_pretrained`: it requires `encoder.onnx`
/// and `decoder_joint.onnx`, plus `tokenizer.model` for the vocab).
const NEMOTRON_FILES: &[&str] = &["encoder.onnx", "decoder_joint.onnx", "tokenizer.model"];

/// Base URL of the Nemotron English ONNX model on Hugging Face. Overridable via
/// `BLUEY_PARAKEET_MODEL_URL` for mirrors / internal artifact stores (enterprise).
///
/// Points at the **int8** export (`lokkju/nemotron-speech-streaming-en-0.6b-int8`),
/// the public source the `parakeet-rs` README lists for the English-only model.
/// int8 is chosen deliberately: it is self-contained (no external
/// `encoder.onnx.data` sidecar, unlike the fp16 export), ~660MB total, and
/// `parakeet-rs` `Nemotron::from_pretrained` loads it directly. The previous
/// default (`altunenes/parakeet_nemotron_en_onnx`) is gated and returns HTTP 401,
/// so first-run auto-download failed; this repo serves the three files publicly.
const DEFAULT_MODEL_BASE_URL: &str =
    "https://huggingface.co/lokkju/nemotron-speech-streaming-en-0.6b-int8/resolve/main";

/// Local filename for the Sortformer (speaker-diarization) ONNX model. The
/// loader ([`parakeet_rs::sortformer::Sortformer::new`]) takes a path, so the
/// on-disk name is our choice; kept as the v2 name the existing presence check
/// used before on-demand download was added.
const SORTFORMER_FILE: &str = "diar_streaming_sortformer_4spk-v2.onnx";

/// Direct download URL for the Sortformer ONNX model, fetched ON DEMAND only
/// when diarization is requested (`BLUEY_STT_DIARIZE=1`) and the ~490MB file is
/// missing — never part of the default first-run download, so the 99% who don't
/// enable speaker labels never pay for it. Overridable via
/// `BLUEY_SORTFORMER_MODEL_URL` (mirror / air-gapped bundle).
///
/// Points at `cgus/diar_streaming_sortformer_4spk-v2.1-onnx` — verified PUBLIC
/// (`gated: false`, file resolves 302→CDN) 2026-07. It ships v2.1; parakeet-rs
/// loads it fine (the loader is version-agnostic, path-driven). The official
/// `nvidia/*` and `onnx-community/*` repos are gated (HTTP 401) and would break
/// auto-download, exactly like the STT model's old `altunenes` default did.
const DEFAULT_SORTFORMER_URL: &str = "https://huggingface.co/cgus/\
    diar_streaming_sortformer_4spk-v2.1-onnx/resolve/main/\
    diar_streaming_sortformer_4spk-v2.1.onnx";

/// Resolve the model directory: env override, else `<data_dir>/models/parakeet-en`.
pub fn resolve_model_dir(paths: &AppPaths) -> PathBuf {
    if let Ok(dir) = std::env::var("BLUEY_PARAKEET_MODEL_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    paths.data_dir.join("models").join("parakeet-en")
}

/// True when every required model file is already present in `dir`.
fn model_present(dir: &Path) -> bool {
    NEMOTRON_FILES.iter().all(|f| dir.join(f).is_file())
}

/// Ensure the Parakeet model is available locally, downloading it on first run.
///
/// Returns the [`ParakeetPaths`] the provider needs, or an error if the model
/// could not be provisioned (caller skips the provider, never crashes). Idempotent:
/// a fully-present model dir (manual / offline-bundle / prior run) skips the network.
pub async fn ensure_parakeet_model(paths: &AppPaths) -> Result<ParakeetPaths> {
    let model_dir = resolve_model_dir(paths);

    if model_present(&model_dir) {
        info!(dir = %model_dir.display(), "parakeet model already present; skipping download");
        return Ok(parakeet_paths(model_dir).await);
    }

    info!(
        dir = %model_dir.display(),
        "parakeet model not found; downloading on first run (one-time, ~600MB)"
    );
    tokio::fs::create_dir_all(&model_dir)
        .await
        .with_context(|| format!("failed to create model dir {}", model_dir.display()))?;

    let base = std::env::var("BLUEY_PARAKEET_MODEL_URL")
        .ok()
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_MODEL_BASE_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900)) // large files; generous cap
        .build()
        .context("failed to build model-download HTTP client")?;

    for file in NEMOTRON_FILES {
        let dest = model_dir.join(file);
        if dest.is_file() {
            continue; // partial prior run — keep what's already there
        }
        let url = format!("{base}/{file}");
        download_file(&client, &url, &dest)
            .await
            .with_context(|| format!("failed to download {file}"))?;
    }

    // Verify the set is complete before declaring success (guards a truncated /
    // mid-download interruption from pinning a broken model dir).
    if !model_present(&model_dir) {
        anyhow::bail!(
            "parakeet model incomplete after download in {}",
            model_dir.display()
        );
    }
    info!(dir = %model_dir.display(), "parakeet model ready");
    Ok(parakeet_paths(model_dir).await)
}

/// Build [`ParakeetPaths`] from a ready model dir.
///
/// Sortformer (speaker diarization) is **OFF by default** and opt-in via
/// `BLUEY_STT_DIARIZE=1`. It is the slow, ~90%-of-compute part of the pipeline
/// (measured: running `diarize_chunk` on every audio chunk pegs ~3 CPU cores and
/// makes the STT worker fall behind real time, so the transcript backlog grows
/// unboundedly — the multi-second-and-climbing lag). For the v1 system-audio path
/// the speaker is always the SOURCE ("They"), so per-chunk diarization buys us
/// nothing and costs us the whole latency budget. Leave it off unless a caller
/// explicitly asks for speaker labels.
///
/// When diarization IS requested and the ~490MB model is missing, it is fetched
/// ON DEMAND here (not in the default first-run download) — so "install and it
/// just works" holds for diarization too, without bloating every install. A
/// failed/interrupted fetch degrades to `None` (transcription still works,
/// diarization stays off) rather than erroring the whole STT bring-up.
async fn parakeet_paths(model_dir: PathBuf) -> ParakeetPaths {
    let diarize_requested = std::env::var("BLUEY_STT_DIARIZE")
        .map(|v| v == "1")
        .unwrap_or(false);
    let sortformer_model = if diarize_requested {
        match ensure_sortformer_model(&model_dir).await {
            Ok(path) => Some(path),
            Err(error) => {
                warn!(
                    "parakeet: BLUEY_STT_DIARIZE=1 but sortformer model unavailable \
                     ({error}); diarization disabled (transcription unaffected)"
                );
                None
            }
        }
    } else {
        // Default path: diarization disabled for latency (see doc comment).
        None
    };
    ParakeetPaths {
        nemotron_dir: model_dir,
        sortformer_model,
    }
}

/// Ensure the Sortformer diarization model is present in `model_dir`, downloading
/// it once on demand. Idempotent: a present file skips the network. Returns the
/// model path on success. Only called when diarization is explicitly requested.
async fn ensure_sortformer_model(model_dir: &Path) -> Result<PathBuf> {
    let dest = model_dir.join(SORTFORMER_FILE);
    if dest.is_file() {
        return Ok(dest);
    }
    info!(
        dir = %model_dir.display(),
        "sortformer diarization model not found; downloading on first diarized run \
         (one-time, ~490MB)"
    );
    tokio::fs::create_dir_all(model_dir)
        .await
        .with_context(|| format!("failed to create model dir {}", model_dir.display()))?;

    let url = std::env::var("BLUEY_SORTFORMER_MODEL_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SORTFORMER_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .context("failed to build sortformer-download HTTP client")?;
    download_file(&client, &url, &dest)
        .await
        .context("failed to download sortformer diarization model")?;

    if !dest.is_file() {
        anyhow::bail!(
            "sortformer model missing after download in {}",
            model_dir.display()
        );
    }
    info!(path = %dest.display(), "sortformer diarization model ready");
    Ok(dest)
}

/// Stream one file to `dest`, writing to a `.part` temp first and renaming on
/// success so an interrupted download never leaves a half-written model file.
async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    info!(%url, "downloading parakeet model file");
    let resp = client
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
        .with_context(|| format!("failed to create {}", tmp.display()))?;
    let mut stream = resp;
    let mut written: u64 = 0;
    while let Some(chunk) = stream
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

    // A near-empty file is almost certainly an error page, not a model.
    if written < 1_024 {
        let _ = tokio::fs::remove_file(&tmp).await;
        anyhow::bail!("download {url} produced only {written} bytes (likely an error response)");
    }

    tokio::fs::rename(&tmp, dest)
        .await
        .with_context(|| format!("failed to finalize {}", dest.display()))?;
    info!(dest = %dest.display(), bytes = written, "parakeet model file ready");
    Ok(())
}
