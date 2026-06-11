# Real Listen STT Safeguard

Date: 2026-06-11

## Problem

Clicking Listen could silently fall back to `AudioPipelineStatus::simulated(...)` when the daemon could not build a real audio runtime. That made a normal user flow show `[dev audio:...]` transcript chunks instead of real microphone/system audio.

## Contract

- Normal user Listen must either:
  - start native microphone/system capture and send audio to managed STT, or
  - show an actionable setup/login error in the overlay.
- Mock transcript generation is allowed only for explicit development/screenshot smoke paths:
  - `BLUEY_AUDIO_SIMULATED_ONLY=1`
  - `CUE_AUDIO_SIMULATED_ONLY=1`
- Customer desktops must never hold static Deepgram/OpenAI STT keys. Logged-in production flow uses `/router/transcribe`.

## Current Managed STT Route

1. macOS native helper captures system and microphone audio.
2. Daemon chunks audio and posts to `bluey-server /router/transcribe`.
3. Server tries Deepgram Nova-3 first.
4. Server falls back to OpenAI transcription when the configured route permits it.
5. Transcript segments return to the overlay/session store with source labels.

Google STT is not currently in the managed route. Add it as a server-side provider before exposing it in product language.

## Verification

- `cargo fmt --all --check`
- `cargo test -p cue-daemon --all-targets`
- `cargo clippy -p cue-daemon --all-targets -- -D warnings`
- `git diff --check`
