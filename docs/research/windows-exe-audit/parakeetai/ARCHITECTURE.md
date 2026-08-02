# ParakeetAI 3.7.0 Windows architecture

## Audio and meeting flow

```text
MMDevice capture-session enumeration (Rust N-API)
              │ 1 second snapshots
              ▼
Electron utilityProcess mic-monitor worker
              │ new/removed allowed meeting processes
              ▼
main-process meeting lifecycle ──► renderer notification/session

mic MediaStream ─┐
                 ├─ Rust Sonora/WebRTC AEC ─ cloud transcription/answers
Chromium loopback┘
```

The exact Windows Rust source initializes COM, enumerates active `eCapture`
endpoints, obtains each `IAudioSessionManager2`, retains active sessions, resolves
their PIDs to process image names, and deduplicates by process/PID/device
(`native-modules/src/platform/windows/ffi.rs:1-185` and
`audio_input/windows.rs:1-27`). The worker polls once per second and posts only
changes (`dist/main/mic-monitor-worker.js`, final worker module).

Main-process allowlists cover browsers and meeting apps. A newly active allowed
process produces a start hint; removal produces an end hint. The state machine
does not visibly require multiple consecutive samples, so browser capture can
produce false positives. Bluey should add debounce and confirmation.

System audio uses Electron display-media loopback and removes the video track.
The native AEC wrapper operates at 16 or 48 kHz with WebRTC/Sonora. It holds an
80 ms microphone delay, uses 10 ms frames, caps each queue at 20 frames/200 ms,
drops older queued audio under pressure, drains at most 12 render and 6 capture
frames per call, and opens a microphone-only bypass after two seconds without a
render reference (`audio_processor/api.rs:1-174`; `engine.rs:1-47`).

The preload exposes generic invoke/on methods plus loopback enable/disable.
BrowserWindow relies on Electron defaults for isolation/node integration, grants
all `media` permission requests regardless of requesting origin, and opens any
requested external URL through the OS (`dist/main/main.js` offsets
395100-396000). This is weaker than the audio design.
