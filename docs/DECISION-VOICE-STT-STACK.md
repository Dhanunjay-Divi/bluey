# Decision — Local Voice / STT Stack: adopt `parakeet-rs`

> Decision note. We will NOT build our own STT/diarization stack and will NOT
> adopt a full voice-agent platform. We adopt one lean Rust crate — `parakeet-rs`
> — for on-device English STT + on-demand diarization, behind our existing
> `SttProvider` trait.

## Context

Bluey needs fast, local, English speech→text with speaker labels, on 16GB
laptops with no dedicated GPU (the local-first USP). Building this from scratch
burns time/credits; full voice-agent platforms (VAPI / EchoKit / Feros / Dograh)
are the wrong shape — they bundle TTS, telephony, dialogue, and their own LLM,
none of which Bluey uses (Bluey listens and hands text to the user's own agent;
it does not speak or place calls). We want the one narrow slice — STT +
diarization — lean and fast.

## Decision

**Adopt [`parakeet-rs`](https://github.com/altunenes/parakeet-rs)** (crates.io
`parakeet-rs` v0.3.6, June 2026) as the on-device STT + diarization backend.

### Why it fits (verified)
- **License:** MIT OR Apache-2.0 (code) — commercial-safe, permissive. NVIDIA
  model weights are separately licensed → verify NVIDIA's model terms for
  commercial use before shipping (open item).
- **English:** a Nemotron **English-only** model (~600M, ONNX). Parakeet beats
  Whisper large-v3 on English accuracy and is ~27× faster on CPU.
- **Compute:** ONNX Runtime (`ort` crate), **CPU-capable by default**, ~real-time
  on a normal laptop CPU (up to ~30× real-time claimed); optional GPU features
  (CUDA / WebGPU-Metal / DirectML). Fits 16GB, no GPU required.
- **Diarization built in:** `Sortformer::diarize_chunk()` — streaming,
  state-preserving, ≤4 speakers, int8-quantized, returns `speaker_id` + span.
  Can run on-demand (the "who is speaking" enabler), not always-on.
- **Input = 16kHz mono f32** — exactly what our `pcm::AudioChunk` already
  produces.
- **Platforms:** macOS/Apple Silicon (Metal via WebGPU EP), Windows, Linux.
- **Maturity:** 333★, actively pushed (June 2026), real versioned crate.

### Key API surface
- STT: `Nemotron::transcribe_chunk(&[f32])` (560ms chunks @16kHz);
  `MultitalkerASR::transcribe_chunk` returns `speaker_id` + `text`.
- Diarization: `Sortformer::diarize_chunk(&audio)` → segments with
  `speaker_id`, `start`, `end`.

## Why NOT the alternatives
- **Build our own** — wasteful; Parakeet already does STT + diarization, faster,
  on CPU. (Also retires the unbuilt `cue-whisper` helper — no longer needed.)
- **Full voice platforms (VAPI/EchoKit/Feros/Dograh)** — wrong shape: TTS +
  telephony + dialogue + their own LLM = bloat Bluey doesn't use. Stripping them
  down is more work and risk than using one lean library.
- **whisper.cpp** — slower on CPU (~3× real-time vs Parakeet's ~30×), no
  diarization, and English accuracy is lower. Multilingual is its only edge,
  which we don't need (English-only for now).
- **Kyutai STT** (2.9k★) — excellent models, but server/GPU-oriented; keep as a
  reference for a future server-grade option, not the on-device bet.
- **vox** (36★) — right shape (Whisper + voice-embedding speaker ID) but too
  young to depend on; **mine it** for the voiceprint/enroll-once embedding
  pattern when we build "who is speaking."

## Integration plan (how it slots in)
Our STT abstraction is the **`SttProvider` trait**
([cue-core/src/stt.rs:164](../crates/cue-core/src/stt.rs)):
`send_audio(chunk)` / `next_event() -> TranscriptEvent` / `finalize` / `close`.
A backend = one struct implementing the trait, registered in the
[STT factory](../crates/cue-daemon/src/stt/factory.rs) chain (mirrors the
existing LocalWhisper provider).

Steps:
1. Add a `ParakeetProvider` implementing `SttProvider`: wrap
   `Nemotron::transcribe_chunk` (English), feed `AudioChunk` (already 16kHz
   mono), emit `TranscriptEvent::Partial/Final`.
2. Run `Sortformer::diarize_chunk` alongside (on-demand) → attach speaker labels
   to transcript events.
3. **Known refactor:** the factory currently only feeds the *continuous
   streaming* path; the default **mic chunk path posts WAV to a REST endpoint**
   and bypasses the trait (flagged in `factory.rs` as a planned "unify the two
   paths" follow-up). Routing mic audio through `SttProvider` is part of this
   work — a known, already-planned unification, not a surprise.
4. Replace the `BLUEY_STT_LOCAL_WHISPER` keyless path with Parakeet as the
   default local provider (no helper binary, no model-download dance beyond the
   ONNX model fetch).

## Packaging — what ships in the binary vs downloads

Two separate artifacts, handled differently:

1. **`ort` / ONNX Runtime native lib** (`.dylib`/`.dll`/`.so`) — **bundled INTO
   the app at build time** (part of the binary; ~MB-to-tens-of-MB,
   platform-specific). Not a runtime download. This is why per-OS Tauri-bundle
   packaging is an open item.
2. **The ~600MB ONNX model weights** (`encoder.onnx`, …) — **NOT in the binary.**
   Default: **first-run download from HuggingFace + local cache**, then 100%
   on-device forever (no per-meeting network, no data leaving). Provide an
   **"offline bundle" build option** that ships the model inside the installer
   for enterprise / air-gapped / regulated customers who cannot fetch it.

**Privacy framing:** "fully local, nothing leaves" is about the user's DATA
(audio/repo/voiceprints never leave) — that holds regardless. A one-time model
*fetch* is a download INTO the machine, not data leaving; standard for every
local-AI app. After first run, fully offline.

## Open items (resolve before ship)
- Verify NVIDIA model-weight license for commercial use.
- Confirm `ort` (ONNX Runtime) packaging on Windows + Apple Silicon in the Tauri
  bundle (native lib shipping).
- Benchmark the EN model's real-world streaming latency on a 16GB Windows laptop
  (the target) to confirm the real-time claim on integrated graphics.
- Decide default model fetch (first-run download + cache) vs the offline-bundle
  build variant; wire a model-download-with-progress + integrity check.

## Net effect
One lean Rust crate (MIT/Apache, on-device, CPU, diarization built in) replaces:
the unbuilt cue-whisper helper, the "which STT" question, and the "how do we
diarize" question. Architecture: **parakeet-rs (English STT + on-demand
Sortformer diarization) → SttProvider → projection assembler → the user's own
agent.** No TTS, no telephony, no second LLM.
