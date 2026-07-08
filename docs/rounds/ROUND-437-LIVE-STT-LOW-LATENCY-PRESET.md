# Round 437 - Live STT Low Latency Preset

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner compared Bluey's live captions with references recommending Deepgram Nova-3, immediate interim rendering, tiny audio chunks, low endpointing, `smart_format=false`, and optional dictionary replacement. The owner also clarified that Indian English should stay primary because accent handling is a major quality requirement.

## Decision

Keep Deepgram Nova-3 as Bluey's production live-caption provider. Do not switch production captions to browser Web Speech. Instead, make the live-caption Deepgram preset faster and keep Indian English as the default language hint.

## What Changed

- Server managed STT relay defaults now favor low visible caption delay:
  - `endpointing=10`
  - `smart_format=false`
  - `utterance_end_ms` omitted by default
  - `interim_results=true`
  - `vad_events=true`
  - `no_delay=true`
  - `language=en-IN`
- Server relay can still opt into the readability preset with env vars:
  - `BLUEY_DEEPGRAM_SMART_FORMAT=true`
  - `BLUEY_DEEPGRAM_ENDPOINTING_MS=150`
  - `BLUEY_DEEPGRAM_UTTERANCE_END_MS=1000`
- Daemon streaming Deepgram defaults now match:
  - `smart_format=false`
  - `endpointing=10`
  - no forced utterance-end wait
- Existing STT repair/dictionary behavior remains in place, including the important correction for:
  - `acetone two numbers` -> `a set of two numbers`
  - `lro cache` -> `LRU cache`
  - `memo is asian` -> `memoization`

## Why

The owner wants captions to feel live while speaking. Interim captions already exist in Bluey's path, but the defaults were still tuned partly for cleaner final text. Forced smart formatting and utterance-end waiting can make captions feel slower. We should show interim text immediately and clean up after finalization, not delay the visible caption for perfect formatting.

## What This Does Not Solve Yet

- The default real-audio loop still has a documented chunked-REST path. Full realtime behavior requires making the streaming relay the normal path for all live mic/system audio.
- Dynamic per-session keyterms from typed text, screen OCR, docs, and recent repairs still need to be fed into the relay beyond the static/default keyterm list.
- Raw/interim/final/repaired transcript comparison still needs to be reviewed from the session audit bundle after live testing.

## Files

- `server/src/api/stt.rs`
- `crates/cue-daemon/src/stt/deepgram.rs`
- `crates/cue-daemon/src/stt/factory.rs`

## Status

Local code changed. No deploy and no GitHub Actions were run in this round.
