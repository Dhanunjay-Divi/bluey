# Final Round 2.4.0 Windows architecture

## Observed system

```text
audio_detect.node ─ device/activity hints
audio_capture.node ─ WASAPI mic + render loopback + WebRTC APM/AEC
keyboard_monitor.node ─ global key hook
             │
             ▼
Electron main services/gateway ─ preload IPC ─ nine React windows
             ├─ XState-like session lifecycle
             ├─ Socket.IO/cloud ASR/answer streams
             └─ reports/documents/video coach/update/telemetry
```

Windows capture defaults are 16 kHz mono PCM16 with 512-sample buffers. The main
declares a 16,384-sample ring, 200 ms target maximum latency, 10 ms callbacks,
10 ms WASAPI duration, 15 ms silence-fill threshold, and expected packet timing
(`out/main/index.mjs:3483-3555`). The Windows adapter implementation begins at
`index.mjs:8585`.
The Windows adapter exposes system+mic capture, optional AEC, silence filling,
pause/resume, and session IDs. Native strings/imports confirm MMDevice/WASAPI
loopback and WebRTC AudioProcessing; source is unavailable.

The session lifecycle is an explicit state machine. Updates and destructive
lifecycle transitions defer while a session is busy. The UI separates pill,
capture halo, audio indicator, interview assistant, coding/system-design,
session configuration, and video-coach concerns into distinct windows. Window
options explicitly set `contextIsolation: true` and `nodeIntegration: false`
(`index.mjs:6129-6241`); sandbox is not explicitly enabled.

The main uses a registered command/query gateway rather than raw channel calls.
Every registered invocation validates `senderFrame`: development callers must be
localhost, while production callers must be `file:` URLs whose parent directory
is one of the known renderer entry names (`index.mjs:6260-6408`). Multiple
windows still share a broad preload capability. Native keyboard monitoring
falls back to Electron global shortcuts if the addon is unavailable
(`index.mjs:17541-17620`). The audio indicator polls every 500 ms with a 300 ms
debounce.

## Updater flow

The updater disables automatic download, defers action during live sessions,
then verifies the downloaded Windows executable with `Get-AuthenticodeSignature`
and requires CN or O `Final Round AI, Inc` before offering restart
(`index.mjs:6767-7089`). Verification failure is fail-closed and reported. The
PowerShell command interpolates a quoted path into script text; Bluey should use
native WinVerifyTrust or a path-safe argument mechanism.
