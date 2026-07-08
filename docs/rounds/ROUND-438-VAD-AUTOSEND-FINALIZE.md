# ROUND-438 VAD Autosend Finalize

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Fix the live-listen path where Bluey could miss the final words of mic/system audio, then Auto-send from an incomplete transcript or not Auto-send at all.

The expected flow is:

- Audio starts.
- Mic/system speech is streamed to STT.
- When speech pauses, transcript text settles.
- Bluey sends once after the transcript has enough content.
- Manual Stop cancels pending Auto-send instead of surprise-sending.
- Logs make it clear whether a final transcript frame reached the desktop after stop.

## Finding

The managed Deepgram relay drains tail frames after stop by sending `CloseStream` and waiting for final frames, but the daemon cleared the active audio session before those tail frames returned.

That meant final STT frames could hit `add_audio_transcript_segment_inner` after stop and be dropped as late, even though they were exactly the final words we wanted.

Overlay Auto-send also only scheduled from final caption events. If Deepgram produced good partials but delayed the final flag, Auto-send could feel stalled.

## Changes

- Added a short daemon-side `finalizing_session` window after audio stop.
- Tail transcript frames are accepted during that finalizing window, but no more audio is forwarded after stop.
- Added local logs for accepted finalizing tail segments with session id, source, final flag, char count, and word count.
- Cleared finalizing state after relay settlement or after a short expiry so stale audio cannot attach later.
- Updated relay session checks to use active-session matching for outbound audio and active-or-finalizing matching for inbound transcript drain.
- Updated macOS overlay Auto-send:
  - Final captions still schedule quickly after 300 ms.
  - Partial captions now schedule after 900 ms of quiet as a fallback when final flags are late.
  - Manual Stop still cancels pending Auto-send.
  - Auto-send scheduling logs now include whether the trigger was final or partial and the delay used.

## Verification

Passed:

- `cargo check -p cue-daemon --quiet`
- `swift build` in `native/macos/cue-overlay`
- `git diff --check`

## Deploy Status

Not deployed. Per owner instruction, this round did not use GitHub Actions and did not publish a release. This is ready for local testing or a later signed deploy when requested.
