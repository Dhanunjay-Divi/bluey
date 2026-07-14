# STT & Diarization — validated findings + port plan

**Status:** research + isolated-probe validation DONE. Production port DONE (2026-07-14): sentence recipe committed earlier; anchor-pinned diarization + overlay speaker labels ported (see §5). This doc exists so we never re-research this.

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
- **VoxConverse: BOTH halves present** — RTTM ground truth at `~/Downloads/voxconverse-master/test/*.rttm` AND the matching **audio** at `~/Downloads/voxconverse_test_wav/*.wav` (232 matched pairs, 2–21 speakers).

### MEASURED (2026-07-14): DER benchmark on VoxConverse (probe: `scratchpad/diar-test`)

Custom DER scorer (10ms frames, 0.25s collar, greedy optimal mapping). Batch = 15
files balanced across speaker counts.

**Round 1 — full files, offline vs the shipping `LiveDiarizer`:**
| | mean DER | spk-count exact |
|---|---|---|
| OFFLINE `Diarizer` | **6.7%** | 8/15 |
| LIVE `LiveDiarizer` (30s windows, time-overlap stitch) | **47.9%** | 3/15 — hallucinated up to **28 speakers** on a 7-speaker file |

→ **Offline speakrs is excellent (matches its claimed 7.8%). The live window-stitcher
is BROKEN and unusable** — every 30s window re-clusters from scratch; failed overlap
mappings mint phantom speakers that compound with meeting length. Do not ship it.
(The earlier "4/6 on 6_speakers.wav" scare was the stress clip — ~5s of speech per
speaker is too little to cluster; on realistic files offline is near-SOTA.)

**Round 2/3 — the ANCHOR-PINNED architecture, DEFINITIVE 4-way result
(first-240s spans, 15 files, 1–11 speakers, empty-ref files excluded):**
| | mean DER | spk-count exact |
|---|---|---|
| OFFLINE (plain full-clip) | 8.7% | 7/15 |
| LIVE-WIN (shipping window-stitcher) | 26.6% (→47.9% on full files; errors compound) | 4/15 |
| **ANCH-LIVE (simple ticks — SHIP THIS for live labels)** | **12.3%** | **8/15** |
| **ANCH-FIN (anchor-pinned final pass — the record)** | **8.2% — BEATS plain offline** | **10/15 (best)** |

Standout per-file: `fzwtp` (11 speakers): ANCH-LIVE found **exactly 11** (offline
found 10; the shipping stitcher: 62% DER). `tpnyf` 5/5, `qadia` 7/7. On
meeting-realistic files (≤4 spk), ANCH-FIN averages **~6%**.

**The validated architecture:** keep a gallery of one ~8s clean "anchor" clip per
known speaker; every tick (~10-30s) diarize
`[anchor₀ + gap + anchor₁ + … + last ~90s window]` as ONE clip with the OFFLINE
clusterer. The cluster containing anchor_i's time-span IS speaker i — identity
pinned by construction (no cross-run centroid matching, no window-overlap
guessing). A window cluster overlapping no anchor = new voice → mint id + cut its
8s anchor. Tick labels only its new span (label-once). Periodically (and at
meeting end) run the SAME pinned pass over the full audio → the authoritative
record (ANCH-FIN), with ids consistent with the live gallery; unmapped clusters
in that pass mint fresh ids (dropping them = 56% miss, was a bug). Constant cost
per tick (~5-8s at CoreML 18×); error does NOT compound with meeting length.

**Tuning that matters (all measured, don't re-learn):**
- **8s anchors + 90s window is load-bearing.** 4-5s anchors or 60s windows →
  the clusterer merges voices (20-36% DER). Anchors pin identity but cannot force
  splits — give the clusterer context.
- **FAILED variants (measured WORSE — do not re-add):** (a) per-tick retroactive
  re-labeling — naive "last tick wins" lets one bad tick destroy 90s of good
  labels; confidence-gating it (tick-level conf from anchors-merged detection)
  still lost. (b) A GLOBAL "under-split pass" guard that suppresses minting —
  with 5+ anchors some pair merges almost every pass, so enrollment stops and
  speaker counts collapse (3-of-5, 4-of-7). (c) Anchor refresh (2nd sub-clip)
  gave no measurable win once retro-relabel was gone.
- The live↔final gap (12.3→8.2) is early-tick labels before full enrollment;
  production closes it with the periodic pinned full-pass re-label (labels are
  already revisable in Bluey's model), NOT with per-tick retro machinery.

### Next diarization step
Port to production in `cue-diarize`: replace `LiveDiarizer`'s window-stitching
with anchor-pinning — gallery + simple label-once ticks (live labels) + periodic
pinned full-prefix pass + end-of-meeting pinned pass (the record). Bluey-specific
boosters: mic channel = "You" needs no diarization (only system audio does);
cross-meeting anchor gallery doubles as the voice-print DB that
`cue-daemon/db/diarize.rs` already stores; optional screen-OCR of the meeting
app's active-speaker tile → real names bound to clusters.
Benchmark harness (keep!): `scratchpad/diar-test` — `diarprobe ascore <wav> <rttm>`
and `abatch <wav_dir> <rttm_dir> [N] [TRIM_S]`; VoxConverse pairs in
`~/Downloads/voxconverse_test_wav` + `~/Downloads/voxconverse-master/test`.

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

## 5. Port plan — DONE (Phases 1–2 shipped; Phase 3 open)

Build/test with `--target aarch64-apple-darwin` and
`PKG_CONFIG_PATH=/opt/homebrew/opt/openblas/lib/pkgconfig`, features
`parakeet-stt local-memory diarize`.

**Phase 1 — sentence-boundary recipe: PORTED + committed** (hold-the-tip +
SentenceAssembler in `cue-daemon/src/stt/parakeet.rs`; Multilingual 3.5 stays
opt-in via `BLUEY_PARAKEET_MODEL_DIR`).

**Phase 2 — anchor-pinned diarization: PORTED (2026-07-14).** What landed where:
- `crates/cue-diarize/src/anchor.rs` — `AnchorLiveDiarizer` (gallery + pinned
  window ticks + minting) + pure `map_raw_to_gallery` with unit tests.
  `ANCHOR_WINDOW_SECS = 90`, 8s pins, 0.75s gaps, 3s enroll / 1s claim minimums
  — all measured, do not shrink (§3). Old `LiveDiarizer` kept but marked
  SUPERSEDED in `lib.rs` docs.
- `crates/cue-daemon/src/diarize.rs` — worker loads `AnchorLiveDiarizer`;
  `live_tick` submits `retention.rolling_window()` (CONSTANT per-tick cost)
  instead of the growing full buffer (which saturated the worker ~15 min in).
  `LIVE_WINDOW_SECS = cue_diarize::ANCHOR_WINDOW_SECS` also sets the retention
  rolling cap. `post_process_meeting` unchanged (plain authoritative offline
  pass + centroid persist).
- Speaker labels → overlay, live: labels lag lines by up to one tick, so the
  daemon pushes `OverlayCommand::TranscriptSpeaker { id: segment_id, speaker }`
  (`cue-core/src/overlay.rs`) via `app.rs::push_transcript_speaker`;
  `app.rs::to_wire_line` maps `speaker_id` → `"Speaker N"` for the
  rehydrate/past-meeting snapshots.
- Overlay UI: `tauriClient.ts` handles `transcript_speaker` →
  `MeetingClient.onSpeakerUpdate`; `transcriptGrouping.ts` gets `setSpeaker`
  (patch by segment id — `GroupedLine.ids` is the hook) + a
  split-on-conflicting-speaker fold rule; `meetingState.tsx` owns the
  subscription; AskScreen/MeetingsScreen render
  `line.speaker ?? (mic ? "You" : "They")`. NOTE: the live transcript card's
  `title` is the CHANNEL ("System"/"Mic"), never a speaker — `onTranscript`
  must keep `speaker: undefined`.

**Phase 2 leftovers (deliberate, small):**
- End-of-meeting pinned full pass through the worker gallery (id-consistent
  record; today's post pass re-clusters from scratch so final ids can differ
  from live ids).
- Cross-meeting anchor gallery (voice-print DB), OCR name binding — boosters
  from §3, not started.

**Phase 3 — the real limitation (still open):**
Investigate a `parakeet-rs` version exposing `att_context_size`/right-context,
or plan a vendored-encoder patch (~480ms lookahead at 8×/80ms-per-frame) to fix
mid-word splits at the model level.
