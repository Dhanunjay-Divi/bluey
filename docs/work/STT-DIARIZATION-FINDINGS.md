# STT & Diarization — validated findings + port plan

**Status:** research + isolated-probe validation DONE. Production port NOT started (except unrelated answer-style work). This doc exists so we never re-research this.

**Date of investigation:** 2026-07 (session on `agent/meeting-build-separation`).

---

## TL;DR

- **The sentence-fragmentation bug** ("are there any" / "other changes from staff" split into separate segments) is a **known, solved problem**. The production recipe was **validated live on real audio** in an isolated probe. Ready to port.
- **We were NOT behind on tech** — Bluey already runs the right models (Nemotron STT, and `speakrs` for diarization). The gaps are **integration/plumbing**, not models.
- **Diarization: we tested the WRONG model** (Sortformer, 4-speaker-capped, from `parakeet-rs`). Bluey's real diarizer is **`cue-diarize`/`speakrs`** (unlimited speakers, 7.8% DER on VoxConverse). The real diarizer has NOT been validated on multi-speaker audio yet.
- **Nothing STT/diarization is in production yet.** The isolated probe lives at `scratchpad/eou-test/` (throwaway).

---

## 1. How production STT actually works (verified, so we don't re-research)

### Sentence boundaries — the universal 2-event model
Every major vendor (Deepgram, AssemblyAI, Speechmatics) uses the same pattern:
- **`is_final`** = "these words are settled" — fires MANY times inside one sentence.
- **`speech_final` / `EndOfTurn` / `UtteranceEnd`** = "a real pause happened" — the DIFFERENT, stronger signal.
- **The bug everyone warns about:** if you start a new segment on every `is_final`, you fragment. **Buffer finals; only close on the pause signal.** This is exactly Bluey's bug.
- Silence thresholds: Deepgram default is 10ms (fragments) → production tunes to **~300–800ms** + a word-gap safety net (~1000ms).

### The 2026 frontier — semantic turn detection
Deepgram Flux (`EndOfTurn`), AssemblyAI semantic endpointing, OpenAI `semantic_vad` score **"is this a complete thought?"** — "because…" keeps waiting, "Thanks." ends. Our **dangling-word gate** is a cheap local version.

### Punctuation
A separate small model (or baked into the model, like Nemotron) inserts punctuation from **word patterns, not intonation** ("are there any…" reads like a question → "?"). Needs ~2–4 words lookahead → live punctuation flickers then settles.

### Real-time diarization
1. Detect end of speaker turn → 2. extract a **voice embedding** (fingerprint) → 3. compare to a **running speaker memory bank** (cosine similarity) → 4. match = known speaker, else new.
- >4 speakers (unlimited): **sliding window + embedding re-ID** — run the model on overlapping windows, match voice fingerprints across windows to stitch a global unlimited speaker set. This is what pyannote/Otter/Fireflies do. **`speakrs` already implements this (VBx+PLDA clustering).**

### Verdict: Bluey is not behind on models
- STT: Nemotron/Parakeet (has punctuation, cache-aware streaming) — same class as vendors.
- Diarization: Sortformer is the most advanced *openly-published* streaming diarizer; `speakrs` is pyannote-parity with clustering. Both modern.
- The gaps were always **integration**, not models.

---

## 2. The validated sentence-boundary recipe (PROVEN in probe, ready to port)

Tested live on real meeting audio (system audio via BlueyAudio helper). Turned **~90 raw fragments → ~12 clean sentences**. The recipe, in order:

| Piece | What it does | Status |
|---|---|---|
| **Multilingual Nemotron 3.5** | The model with the punctuation head — gives real commas/periods/caps. Beats English-only Nemotron (which produced run-ons). | ✅ validated |
| **Hold-the-tip** (Fix 1B) | Hold back the LAST word of each 560ms chunk as provisional; reunite with next chunk before committing → "month"+"ly" = "monthly". Fixes mid-word splits. | ✅ validated |
| **Silence gate + dangling-word check** | Buffer text; close a sentence on ~500ms silence UNLESS it ends on a mid-thought word (I, the, was, and, to, of…). 1500ms hard ceiling so it never hangs. | ✅ validated |
| **Coalesce-not-drop** (Fix 2) | A fragment <2 content words (or only backchannels: yeah/no/okay/mhm…) ATTACHES to the prior sentence, never stands alone, never dropped. | ✅ validated |

### Models used (already downloaded)
- English Nemotron: `~/Documents/antigravity/nifty-brahmagupta/realtime/nemotron/` (encoder+data+decoder+tokenizer)
- **Multilingual Nemotron 3.5** (RECOMMENDED — punctuation): `scratchpad/eou-test/models/nemotron-3.5-asr-streaming-0.6b-onnx/` — from HF `altunenes/parakeet-rs/nemotron-3.5-asr-streaming-0.6b-onnx`

### Things ruled OUT (don't retry)
- **`ParakeetEOU` model** — FAILED. Emitted one 60s blob with zero boundaries; author's own README says "does not work very well". Dead end.
- **Cloud STT for the intelligence layer** (summary/decisions/question-detection) — post-hoc, ungrounded, and duplicates what the user's agent already does live+free. See `docs/work/` cloud analysis if written. Cloud STT is only worth it for weak-machine transcription tiers.

### The one REAL limitation (not a tunable)
- **Mid-word split root cause (Fix 1A):** `parakeet-rs 0.3.6`'s Nemotron encoder is compiled FULLY-CAUSAL (`CHUNK_SIZE=56`, `PRE_ENCODE_CACHE=9`, ZERO future right-context, no `att_context_size` surface). The true fix (model right-context lookahead ~480ms) needs a **crate upgrade or a vendored-encoder fork** — NOT a config change. Hold-the-tip (Fix 1B) is the local mitigation and works. Schedule 1A separately.

---

## 3. Diarization — UNRESOLVED, test the RIGHT model

### What went wrong in the investigation
The probe used **Sortformer** (`parakeet-rs::sortformer`), which is **hard-capped at 4 speakers** (`NUM_SPEAKERS=4`, `set_max_speakers` clamps to 4). On a 3-speaker clip it spawned phantom Speaker 2/3/4 on crosstalk. An early hack (`MAX_SPEAKERS=2` hard cap) was WRONG — it merged a real 3rd speaker. **Do not cap speakers.**

### The correct diarizer (already in the repo, NOT yet tested here)
**`cue-diarize`** crate → wraps **`speakrs` 0.4** (pyannote-parity VBx+PLDA, pure Rust):
- **Unlimited speakers** (real clustering, not 4-capped)
- **~7.8% DER on VoxConverse test** (measured by the crate) — the exact dataset we have RTTM labels for
- CoreML, ~18× realtime on Apple Silicon
- `Diarizer` (offline, authoritative, full clustering) + `LiveDiarizer` (streaming, stitches 30s windows)
- Wired behind `cue-daemon --features diarize` (off by default: links OpenBLAS + CoreML, first-run model download)

### Known caveat of the LIVE path (from the crate's own comments)
`LiveDiarizer` stitches windows by **TIME OVERLAP, not centroid matching** — because speakrs per-run embeddings aren't comparable across runs. So it keeps speakers stable *within* the rolling window, but a speaker who goes silent and returns MAY get a new ID. The offline `Diarizer` uses full clustering and is authoritative.

### Anti-phantom tuning (if we ever tune Sortformer, which we probably won't since speakrs is better)
- onset 0.75 (up from 0.641), min_duration_on 0.35, min_duration_off 0.45, median_window 15
- activity-floor filter (count a speaker only with >~1.5s cumulative talk), A-B-A collapse
- **NO hard speaker cap**

### Test data available
- **`6_speakers.wav`** (16kHz mono, 41s, 6 speakers): `~/Documents/antigravity/nifty-brahmagupta/6_speakers.wav`
- **VoxConverse RTTM ground truth** (232 files, 2–13 speakers): `~/Downloads/voxconverse-master/{dev,test}/*.rttm` — **AUDIO NOT downloaded** (Oxford zip: `robots.ox.ac.uk/~vgg/data/voxconverse/data/voxconverse_{dev,test}_wav.zip`)

### Next diarization step (NOT done)
Test `cue-diarize`/`speakrs` (offline + live) on `6_speakers.wav` → does it correctly find 6? Then decide streaming approach. Only after that, touch production diarization.

---

## 4. Where the code lives

### Isolated probe (throwaway, NOT production)
`/private/tmp/claude-501/.../scratchpad/eou-test/`
- `probe.rs` — 3-lane comparison (raw Nemotron / English+recipe / Multilingual3.5+recipe) + Sortformer + WebSocket
- `frontend.html` — live 3-column viewer (http://127.0.0.1:8788/frontend.html, static server on :8788, ws on :8787)
- `probe-results.log` — last run's output
- run: `BlueyAudio.app/.../BlueyAudio --continuous | probe <en_nemo_dir> <multi_nemo_dir> <sortformer.onnx>`

### Production files the port will touch (per grounded research)
- `crates/cue-transcribe/src/engine.rs` — `SttEngine::push` → `Nemotron::transcribe_chunk` (swap to Multilingual 3.5; add hold-the-tip)
- `crates/cue-daemon/src/stt/parakeet.rs` — `run_worker`, `TranscriptEvent::Final` emit (assembler seam)
- `crates/cue-daemon/src/app.rs` — TWO transcript commit seams:
  - `TranscriptAdd` handler (~:2114) — headless-testable via `bluey listen`
  - `add_audio_transcript_segment_inner` (~:7158)
  - existing merge helper: `dedup_partial_on_final` (~:711, uses `.rposition` same-speaker) — reuse for coalesce
- `crates/cue-daemon/src/diarize.rs` — diarization orchestration (uses `cue-diarize`)
- **Invariant (project memory):** NEVER VAD-gate the continuous 100ms engine feed (holes desync the cache-aware model). All assembly is POST-model, per-source.

---

## 5. Port plan (NOT started)

Do on a FRESH branch (`agent/stt-boundary-fixes`), per git rules. Build/test with `--target aarch64-apple-darwin`.

**Phase 1 — sentence-boundary recipe (validated, do first):**
1. Swap English Nemotron → Multilingual 3.5 in `cue-transcribe` (better punctuation).
2. Add hold-the-tip buffer per-source in `parakeet.rs` `run_worker`.
3. Add the assembler (silence gate + dangling-word + coalesce-not-drop) into BOTH commit seams in `app.rs`, reusing `dedup_partial_on_final`'s same-speaker lookup. Test the IPC path first via `bluey listen`.

**Phase 2 — diarization (validate first, then port):**
4. Test `cue-diarize`/speakrs on `6_speakers.wav` (offline + live). Confirm it finds all speakers unlimited.
5. Only then wire/tune the daemon's `diarize.rs` live path.

**Phase 3 — the real limitation (schedule separately):**
6. Investigate a `parakeet-rs` version exposing `att_context_size`/right-context, or plan a vendored-encoder patch (~480ms lookahead at 8×/80ms-per-frame) to fix mid-word splits at the model level.
