# Production-Grade Transcription + Diarization + Stitching Plan

> Grounded in web research on how Deepgram, AssemblyAI, Speechmatics, WhisperX,
> pyannote, NVIDIA NeMo, and Google/AWS transcribe actually do streaming
> stitching, diarization, and word→speaker alignment — mapped onto Bluey's code.

## Live-tier stability fix (the big one — found by driving REAL audio)

Feeding real VoxConverse audio through the full live daemon exposed the live tier
collapsing all early segments to one speaker, then churning ids
(`{0}→{1,2}→{1,2,3}` for a 3-speaker file). Root cause, measured directly:

- **speakrs's per-run speaker centroids are NOT comparable across separate
  `diarize()` calls** — the same voice measures ~**0 cosine self-similarity**
  across two runs (`A0×B0 = -0.048`). So the old nearest-centroid inheritance
  could never match a speaker to itself across windows → constant re-minting.

**Fix (shipped):** the live tier maps each new window's raw speakers to the
previous window's global ids by **TIME OVERLAP** of the shared audio prefix (the
same region re-diarized keeps the same speaker), not by centroid. IDs are
append-only and arrival-ordered; a raw speaker overlapping no prior id is a new
voice → next id. Validated on real VoxConverse: arrival order now
`{0}→{0,1}→{0,1,2}` stable, correct speaker counts (e.g. gyomp 5/5), final turns
map 1:1 to ground truth. `MIN_ENROLL_SECS=2.0` cold-start guard; 30s rolling
window / 10s step (diart chunk+link topology). See `crates/cue-diarize/src/lib.rs`
`LiveDiarizer`. (Technique: diart chunk+link + NeMo AOSC arrival-order, adapted
because speakrs gives no cross-run-stable embeddings.)

## The three real defects (found by research, verified in code)

The system was further along than expected: `label_segments_by_overlap` already
keys off an audio clock, not wall-clock. So the *original* framing ("audio-seconds
vs wall-clock") was not the live bug. The real defects:

1. **`audio_start_secs` was read at the wrong instant.** It was taken from
   `retention.duration_secs()` at the moment the STT *text arrives*. Nemotron/
   Parakeet is streaming — a final lags the audio by ~0.56s — so the value
   **overshoots** the true speech position by the model latency, enough to cross a
   turn boundary in fast dialogue. Root cause = a clock *offset*, not a *mismatch*.
   **FIXED (P0):** subtract `STT_LAG_SECS` from the arrival read on the audio path.

2. **The second ingest path dropped the clock entirely.** `TranscriptAdd`
   (`app.rs`) built segments with no `audio_start_secs`, so diarization skipped
   every IPC / `bluey listen` segment → "They" forever. This violated the
   two-transcript-paths rule. **FIXED (P0):** stamp the clock on the IPC path too.

3. **Point-containment assignment, not overlap.** The matcher tested only the
   segment's *start* point against a diarized turn. A ~560ms segment can straddle a
   boundary. **P1:** upgrade to WhisperX max-total-overlap.

## What production does (the techniques we're adopting)

- **Two-stream join on one audio clock** — ASR word timestamps × diarizer turns,
  same sample-0 origin. Never wall-clock/arrival-time (corrupted by VAD lookahead
  + model latency). *[all vendors; the load-bearing rule]*
- **Max-total-overlap assignment** (WhisperX `assign_word_speakers`): per diarized
  speaker accumulate `min(end)−max(start)` overlap with the word/segment interval,
  assign argmax; nearest-midpoint fallback (capped) only when zero overlap.
- **Committed prefix + mutable tail** for partials: overwrite the tail each interim,
  never append; freeze on final. (LocalAgreement family.) Bluey broadcasts only
  finals today, so no duplication yet — but build the primitive before partials ship.
- **Boundary = endpointing ∪ speaker-change**, not a fixed pause. Deepgram
  `UtteranceEnd` measures the gap on **word-end timestamps** (~1s); AssemblyAI turn
  detection; ~700ms end-silence with a 250ms min-speech filter. Never a 2.5s wall
  timer.
- **Stable ids via arrival-ordered speaker cache** (AssemblyAI cache / Sortformer
  AOSC): first speaker = 0, held fixed; new centroid → next free id, never reorder.
  Bluey's nearest-centroid inheritance is this pattern — validate it's arrival-ordered.
- **Live is final-on-emit; correction comes from a post re-cluster** (Deepgram/
  AssemblyAI streaming, ~5–15 DER worse than batch, accepted). Bluey's two-tier
  (live tick + post pass) matches this.

## Staged roadmap

### P0 — make labels correct (DONE)
- Subtract `STT_LAG_SECS` (0.56s) from the audio-path `audio_start_secs` read so the
  point lands on the spoken audio, not the arrival instant. `app.rs`.
- Stamp `audio_start_secs` on the `TranscriptAdd` IPC path (the second, headless-
  testable path). `app.rs`.
- Matcher already uses `audio_start_secs` with containment + nearest-fallback.
  `diarize.rs` (live + post).
- **Verify:** `BLUEY_DIARIZE=1`, real multi-speaker audio → lines flip They→Speaker N.

### P1 — production stitching + assignment
- `label_segments_by_overlap` → **max-total-overlap**: give each transcript segment
  an interval `[audio_start, audio_start + audio_dur]` (add `audio_dur_secs` to
  `TranscriptSegment`; estimate from `SttSegmentMetadata.time.duration_ms` or chunk
  span), accumulate overlap per speaker, assign argmax; nearest-fallback capped at
  ~2s else leave `None`. *[WhisperX]*
- Dev view + overlay: replace `PAUSE_MS = 2500` with **new line on speaker-change OR
  audio-time word-gap ≥ ~700ms–1s**, using the audio clock not `ts_ms`. Committed-
  prefix + mutable-tail; keep raw concat (SentencePiece boundaries). *[Deepgram/
  AssemblyAI]*
- Post pass **re-broadcasts** changed labels (`broadcast_speaker_update`) so open
  views reflow to authoritative labels. *[Google CHI gated reflow]*

### P2 — word-level alignment + live↔post stability
- If Parakeet exposes token/word timestamps, join per **word** (splits a segment
  that straddles a turn). *[WhisperX word-level]*
- Live↔post **centroid id remap** (greedy/Hungarian) so numbers don't shuffle at
  meeting end. *[stable-id]*
- Confirm `LiveDiarizer` inheritance is strictly arrival-ordered. *[AOSC]*

### P3 — polish
- Speechmatics sentence-boundary label smoothing (kill single-segment flicker).
- Real Silero VAD endpointing (0.5 threshold, 100ms close-silence, 30ms frames)
  feeding the boundary signal.
- Smart-formatting (punctuation/truecasing/ITN) as a replace-the-tail-before-freeze
  stage. *[Deepgram smart_format]*

## Key files
- `crates/cue-daemon/src/app.rs` — `add_audio_transcript_segment_inner` (audio path
  clock read, STT_LAG_SECS); `TranscriptAdd` handler (IPC path clock).
- `crates/cue-daemon/src/diarize.rs` — `label_segments_by_overlap` (live) +
  `post_process_meeting` (post) matching; `broadcast_speaker_update`.
- `crates/cue-daemon/src/audio/retention.rs` — the shared sample clock.
- `crates/cue-core/src/meeting.rs` — `TranscriptSegment.audio_start_secs`
  (+ future `audio_dur_secs`).
- `crates/cue-core/src/audio.rs` — `SttSegmentMetadata.time` (unused on streaming
  path; word-timestamp source if Parakeet exposes it).
- `dev-live-view/index.html` — stitching + `applySpeakerUpdate` (P1 rewrite).
