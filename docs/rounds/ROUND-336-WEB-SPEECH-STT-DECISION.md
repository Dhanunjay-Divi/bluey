# Round 336 - Web Speech STT Decision

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner asked whether Bluey should implement the browser Web Speech API instead of Deepgram because live captions still feel slower and less accurate than expected.

## Decision

Do not replace Deepgram with Web Speech for Bluey's production desktop overlay STT.

Use Web Speech only as an optional browser/web-dashboard experiment if we want an instant local-looking mic preview for browser sessions. It should not own production billing, transcript history, system-audio capture, or native overlay captions.

## Why

- Bluey's main listener is native desktop audio: microphone plus system audio. Web Speech lives inside a browser/WebView recognition service and is not a clean replacement for the native daemon audio pipeline.
- Web Speech browser support is not dependable enough for production cross-platform desktop STT. MDN marks `SpeechRecognition` as limited availability because it does not work in some widely used browsers.
- Some browsers send recognition audio to a browser-managed web service. That makes provider choice, billing, logging, accuracy tuning, privacy language, and incident debugging less controllable than Bluey's server relay.
- Web Speech can return interim and continuous results, which is useful for UX, but it does not solve Bluey's core requirements by itself:
  - account-scoped usage accounting
  - server-side session IDs and support refs
  - system-audio transcription
  - Deepgram/OpenAI/local fallback routing
  - provider health and 429 handling
  - dynamic keyterms for code/interview vocabulary

## Recommended Path

1. Keep production STT on the managed provider chain:
   - Deepgram realtime relay first
   - OpenAI realtime fallback where enabled
   - local Whisper fallback where enabled
2. Unify all live mic/system paths onto the streaming provider chain instead of leaving any path on chunked file transcription.
3. Add dynamic keyterms from:
   - latest typed question
   - screen OCR/code text
   - attached document names and extracted role/domain terms
   - recent failed transcript repairs
4. Add confidence-aware transcript repair after provider finals, using local deterministic repairs first and a tiny model only when confidence is low and the phrase is answer-critical.
5. Add a browser-only Web Speech spike later behind a clear flag, only for:
   - web dashboard mic preview
   - comparing latency against provider STT
   - not billing or saving as trusted transcript unless explicitly promoted

## Sources Checked

- MDN `SpeechRecognition`: limited availability and browser/service caveat.
- MDN `SpeechRecognition.interimResults`: interim partial result support.
- MDN `SpeechRecognition.continuous`: continuous recognition mode.

## Product Copy Rule

If Web Speech is ever exposed, label it clearly:

`Browser preview captions`

Do not call it production transcript, meeting transcript, or Bluey managed STT unless the transcript is routed through the same account/session/billing/logging path as other STT.

## Status

No product code changed in this round. This is an engineering decision so we do not spend time building a browser-only path that will not fix the native overlay problem.
