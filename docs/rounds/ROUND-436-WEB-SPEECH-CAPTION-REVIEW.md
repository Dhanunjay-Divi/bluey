# Round 436 - Web Speech Caption Review

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner shared Chrome/Web Speech references and caption projects, then asked whether any of them can make Bluey live transcription better than the current Deepgram path.

## Sources Checked

- Chrome Web Speech API introduction: `https://developer.chrome.com/blog/voice-driven-web-apps-introduction-to-the-web-speech-api`
- Web Speech API draft: `https://webaudio.github.io/web-speech-api/`
- `arellanojeremypaul/live-caption-saver`
- `yashrajbharti/captions-on-the-fly`
- `josephdadams/LiveCaption`
- Deepgram interim results, endpointing, utterance end, streaming, and finalize docs.
- Existing Bluey STT code:
  - `crates/cue-daemon/src/stt/deepgram.rs`
  - `crates/cue-daemon/src/stt/factory.rs`
  - `crates/cue-daemon/src/app.rs`
  - `server/src/api/stt.rs`

## Decision

Do not replace Bluey's production desktop transcription with browser Web Speech.

Use Web Speech only as an optional browser-side comparison path or Try Us/dashboard preview later. It is useful inspiration for realtime UI behavior, but it does not solve Bluey's native desktop requirements by itself.

## Why Web Speech Helps

- It shows interim text while the user is still speaking.
- It has a simple browser event model with `continuous` and `interimResults`.
- It is useful for a browser-only caption preview, demo, or latency comparison.
- The linked caption repos are good UI/UX references for saving, displaying, and relaying live captions.

## Why Web Speech Is Not Enough

- Bluey's core listener is native desktop audio: microphone plus system audio. Web Speech is a browser API, not a full native daemon audio pipeline.
- Bluey needs account-scoped billing, session IDs, support refs, R2 audit bundles, provider fallback, and synced transcript history.
- Web Speech does not give us the same server-side control over provider choice, request IDs, usage accounting, phrase/keyterm tuning, or retry behavior.
- The Web Speech spec explicitly treats the underlying recognition engine as implementation-defined. That means browser behavior can vary by browser, OS, and deployment context.
- It does not fix current Bluey issues where some local audio paths can still be chunked/final-only instead of true streaming partials.

## Current Bluey Findings

- Bluey's Deepgram realtime relay already requests:
  - `interim_results=true`
  - `endpointing=200`
  - `utterance_end_ms=1000`
  - `vad_events=true`
  - `language=en-IN` by default
  - default and custom `keyterm` support
- The desktop streaming provider factory defaults Deepgram to `nova-3` and `en-IN`.
- There is still a documented split in `crates/cue-daemon/src/stt/factory.rs`: the default real-audio chunk path can call `transcribe_audio_file()` instead of using the streaming provider chain.
- That split explains why Bluey can feel slower than Web Speech even though Deepgram supports interim realtime captions.

## Best Build Path

1. Make the streaming relay the default for all live Listen paths, including mic and system audio.
2. Keep chunked REST transcription only as a fallback, not the normal live-caption path.
3. Render interim partials immediately in the horizontal live caption preview.
4. On stop/auto-send, send Deepgram `Finalize`, wait for final frames, then send the answer only after all received audio has been transcribed or timed out with a visible warning.
5. Add dynamic keyterms per session from:
   - current typed text
   - recent screen OCR/code terms
   - attached filenames and extracted document terms
   - recent transcript corrections
   - coding/interview vocabulary
6. Keep raw transcript, provider final transcript, repaired transcript, confidence, timings, and audio-chunk refs in the session audit bundle.
7. Add an STT quality/latency dashboard per session:
   - first audio byte
   - websocket opened
   - first interim
   - first final
   - finalize sent
   - finalize completed
   - provider error/reconnect count
8. Add optional second-pass repair for low-confidence final text, using deterministic repairs first and a small model only when needed.
9. Run a browser Web Speech comparison later as a measurement path, not as the source of truth.

## UX Rule

Users should see captions as soon as partial text arrives. The live preview can show changing text, but the sent question must use the finalized or repaired transcript so we do not send half-heard audio.

## Status

No product code changed in this round. This round records the caption architecture decision and the next concrete STT work items.
