# Live Captions Realtime Fix For Kiro Review

## Summary

Bluey's managed Deepgram path is working, but the macOS overlay Listen flow was not true
Deepgram websocket captioning. The current production path captures short native audio
chunks and sends them to `bluey-server` `/router/transcribe`, which proxies to Deepgram
Nova-3. That can feel near-live, but it is not word-by-word streaming.

This round improves the current path without changing the server contract:

- Mic and system chunks are captured/transcribed in parallel instead of sequentially.
- The overlay now shows active Listen animation/state while capture is running.
- Empty/no-speech chunks no longer make the UI look idle; Listen remains visibly active.

## Verification

Manual smoke on uno:

- Native system helper captured non-silent 16 kHz mono PCM while `say` played audio.
- The captured system chunk posted to `https://bluey.sh/router/transcribe` returned:
  `Bluey system caption test one two three.`
- Normal `bluey audio start` with generated system speech emitted transcript segments:
  `transcript segments emitted: 2`, system and mic chunks both advanced to 4.
- Visible overlay screenshot showed transcript text in the live strip after the run:
  `/tmp/bluey-ui-check/overlay-caption-parallel.png`.

## Important Product Note

Deepgram supports true realtime captioning, but Bluey still needs a managed streaming STT
proxy for that mode:

`desktop -> bluey-server websocket -> Deepgram websocket -> overlay`

Until that lands, Bluey uses faster parallel chunked transcription. The chunked path is
safer for alpha because provider keys stay server-side and audio files remain transient,
but it will not match websocket first-word latency.

## Files

- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

