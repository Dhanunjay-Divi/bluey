# Cluely 2.0.193 Windows architecture

## Observed flow

```text
SoX microphone ─┐
                ├─ Electron main ─ generic preload IPC ─ React renderer
Chromium loopback┘                                      ├─ local Silero VAD
desktopCapturer screenshot ─────────────────────────────┤
                                                       └─ cloud RPC/WebSocket
```

The Windows main process prepends the packaged SoX directory to `PATH`, starts
`node-record-lpcm16` at 48 kHz mono, and forwards PCM chunks to the renderer.
For system audio it installs a default-session display-media handler, chooses
the first screen source, and returns `audio: "loopback"`; the renderer requests
display media and drops the video track. This is observed at
`dist-electron/main.js` offsets 501656-503000. The macOS-only AudioTee process is
packaged but rejects non-Darwin platforms.

Renderer VAD creates bounded speech segments and cloud transcription requests.
Session/chat state uses cloud RPC plus a reconnecting agents WebSocket. Overlay,
settings, auth, notification, and dashboard windows are always-on-top/content-
protected according to shared state. Navigation and redirects are restricted to
the packaged renderer origin and external opens are limited to HTTPS/mailto
(`main.js` offsets 494128-495100).

The preload is only 314 bytes but grants generic channel-level IPC. Main handlers
perform a sender-origin check, which helps, but the bridge remains broader than
Bluey's typed daemon protocol. No Windows service or durable local queue is
present; timers drive heartbeats, transcription, calendar/session refresh, and
hourly update checks.

## Recovery behavior

The app stops microphone streams when session state clears and terminates the
macOS helper with a five-second escalation path. The Windows loopback path is a
renderer-owned MediaStream rather than an independently supervised native
process. No device-change recovery journal, lease, or durable local replay queue
was observed.
