# Diarization Integration Plan (speakrs two-tier)

Validated: speakrs = pyannote-parity (7.8% DER full VoxConverse), pure Rust,
CoreML 18× realtime. Live tier (sliding-window + centroid inheritance) = ~11%
DER at constant ~3s latency; post-tier (full VBx) = ~6%. Both use speakrs.

## Prerequisite (blocker for everything)
**Audio is not retained today** — `system_capture.rs` streams 20ms chunks → text,
nothing keeps the recording (agent-confirmed). speakrs needs the whole buffer.
→ **P0: retain the meeting's 16kHz mono f32 audio** (rolling in-memory Vec for
live windows + full buffer/WAV for the post pass).

## Architecture
```
capture (16k f32 chunks) ──┬──► existing STT (Parakeet) → transcript text
                           └──► NEW: audio buffer (rolling + full)
                                       │
   LIVE tier (every ~30s): last 120s window → cue-diarize (speakrs+CoreML)
        → centroid inheritance → stable speaker_id per turn → DB + live event
                                       │
   POST tier (on MeetingEnd): full buffer → cue-diarize full VBx
        → authoritative speaker_id → rewrite transcript rows → AI context
```

## New crate: `cue-diarize`
Isolated (own ort rc.12 — matches daemon). Wraps speakrs + inheritance.
- `Diarizer::from_models(dir, ExecutionMode)` — CoreML default, CPU fallback.
- `diarize(&[f32]) -> Vec<Segment{start,end,speaker,centroid}>` (batch/post).
- `LiveDiarizer` — holds stable centroids; `push_window(&[f32], offset) ->
  Vec<Segment>` with stable ids via nearest-centroid inheritance (thr 0.70).
- Deps: speakrs 0.4 `{online, coreml, openblas-system}` + openblas (bundled).

## DB (migration 010_diarization.sql — 010 is free; 011 keybinds exists)
`transcripts.speaker_id INTEGER` ALREADY EXISTS → live writes it directly.
New:
- `utterance(id, session_id FK cascade, start_ms, end_ms, source,
   speaker_prov INT, speaker_final INT, embed_dim, embedding BLOB, created_at)`
- `meeting_speaker(session_id FK, speaker_final, centroid BLOB, embed_dim,
   n_utterances, total_ms, updated_at, PRIMARY KEY(session_id, speaker_final))`
Reuse existing `speakers(session_id, speaker_id, name, color)` for human names.
Migrations are `include_str!` in `db/mod.rs:61-101` — register there.

## Wiring points (agent-confirmed file:line)
- Audio retain: `audio/system_capture.rs:238-290` (read loop) + the streaming
  task `app.rs:1074-1196` — tee chunks into a buffer on the daemon.
- Live diar task: new tokio task alongside `app.rs:1109` select loop; timer every
  ~30s → diarize last 120s → map turns by timestamp overlap → set speaker_id.
- Segment speaker_id: `TranscriptSegment` (`cue-core/meeting.rs:54-72`) gains
  `speaker_id: Option<i64>` (orthogonal to the coarse `Speaker` enum). Write it
  in BOTH `add_audio_transcript_segment_inner` (app.rs:5396) AND `TranscriptAdd`
  (app.rs:1375) — the two-transcript-paths rule.
- Post hook: `MeetingEnd` handler `app.rs:1342-1374`, after `generate_recap()`
  (~line 1354) → spawn async full-diarize → update DB + transcript rows.
- AI context: `last_transcript_text_bounded` (`cue-core/meeting.rs:330-370`)
  formats `speaker.display_label()`; extend to prefer resolved speaker name /
  `Speaker N` when a speaker_id is present. Live triggers stay on `is_me()`
  (channel-based, unaffected).

## Bundling (decision: bundle everything)
- Vendor speakrs ONNX + PLDA models into `<data_dir>/models/diarize` via a
  `model_setup`-style provisioner (mirror `model_setup.rs:44-112`); OR bundle in
  the .app and point `SPEAKRS_MODELS_DIR` at it.
- Link arm64 OpenBLAS: add to `scripts/build-macos.sh` + stage the dylib into the
  bundle, or use Accelerate. Feature-gate `diarize` in cue-daemon Cargo.toml.

## Cleanup (after integration verified)
Delete throwaway: `dev-diarize-probe/`, `dev-transcript-view/`, `dev-stt.sh`,
`dev-speakrs-probe/` (keep until cue-diarize proven). `models/diarize/` pyannote-rs
test models can go. Scratch in /tmp is ephemeral.

## Task graph (parallelizable)
- T1 (foundation, blocks rest): `cue-diarize` crate — speakrs wrap + LiveDiarizer.
- T2 (parallel): DB migration 010 + `db/diarize.rs` access methods.
- T3 (parallel): audio retention (buffer tee).
- T4 (needs T1+T3): live diar task wiring.
- T5 (needs T1+T2): post-process MeetingEnd hook.
- T6 (needs T2): TranscriptSegment.speaker_id + both write paths + AI context.
- T7 (parallel): model provisioning + build/bundle + OpenBLAS.
- T8 (last): cleanup + build + verify.
