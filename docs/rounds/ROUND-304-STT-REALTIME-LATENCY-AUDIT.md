# ROUND-304-STT-REALTIME-LATENCY-AUDIT

Date: 2026-07-02
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## User Issue

Live captions feel delayed compared with browser Web Speech API captions. The user notices that reading aloud does not appear to transcribe in real time.

## Findings

- The managed Deepgram realtime URL already requests streaming partials:
  - `interim_results=true`
  - `endpointing=300`
  - `utterance_end_ms=1000`
  - `vad_events=true`
- The daemon forwards provider partials to the overlay through `TranscriptPartial`.
- The macOS overlay receives `transcript_partial` and calls `appendLiveTranscript(... final: false)`.
- The Windows overlay also has `transcript_partial` and `transcript_final` display paths.
- Live relay audio chunk reads are small enough for low latency in the happy path:
  - `4096` bytes of 16 kHz PCM16 mono is about 128 ms.
- The current startup path intentionally waits for audible audio before creating the paid STT session and opening the relay websocket.
  - This avoids reserving/charging for silence.
  - It can add a visible startup delay compared with Web Speech, which is usually already hot and emits unstable interim text aggressively.

## Current Likely Cause

The delay is most likely not because partial captions are disabled. The bigger latency contributors are:

- first-audible gating before reservation/websocket open,
- the extra network hop through Bluey's managed relay,
- provider partial timing,
- local audio thresholding when mic/system volume is low,
- lack of enough timing logs to prove which leg is slow on each user machine.

## Expected Product Target

- First visible partial: roughly 300-900 ms after real speech starts.
- Final stabilized caption: roughly 1-2 seconds after a pause.
- Anything that waits multiple seconds before the first partial should be treated as a bug or tuning issue.

## Recommended Next Build

Add an explicit low-latency STT mode:

- Open the managed relay earlier when Listen starts.
- Keep zero-audio settlement/refund behavior so users are not charged when no audio is forwarded.
- Forward partials immediately and keep the horizontal caption strip moving with interim text.
- Add timing diagnostics:
  - listen_clicked_at
  - first_pcm_at
  - first_audible_at
  - stt_reservation_created_at
  - websocket_opened_at
  - first_provider_partial_at
  - first_overlay_partial_at
  - first_final_at

## Verification Performed

Read and checked:

- `server/src/api/stt.rs`
- `crates/cue-daemon/src/stt/deepgram.rs`
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `crates/cue-core/src/vad.rs`
- `crates/cue-core/src/pcm.rs`

No product code was changed in this round.
