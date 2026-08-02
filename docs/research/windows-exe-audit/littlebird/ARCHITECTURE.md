# Littlebird 0.81.10 Windows architecture

## Observed system

```text
Windows UI/apps/audio
        │
        ▼
signed littlebird-capture.exe (.NET 8)
 window/screen/OCR + WASAPI mic/loopback
        │ framed JSON on stdin/stdout
        ▼
Electron main ─ typed preload ─ TanStack/React UI
        ├─ category seed / Electron stores / IndexedDB
        └─ Littlebird API, MCP, telemetry, integrations
```

The main selects `resources/bin/littlebird-capture.exe` on Windows
(`dist-electron/main/index.js:2799-2807,3024-3026`) and spawns it with
`--daemon`, piped stdio, the app version, category database path, and inherited
environment (`index.js:9128-9163`). Messages are delimiter-framed JSON; inbound
objects are schema-decoded before dispatch (`index.js:8760-8804,9230-9400`).

The helper's static metadata identifies a .NET 8 Windows 10.0.19041 build. Its
imports and managed symbols show window/screen capture, OCR, `NAudio.Wasapi`,
`WasapiCapture`, `WasapiLoopbackCapture`, active audio sessions, and stdout
serialization. Named-pipe strings are present in the .NET runtime/diagnostics
surface, but the observed application protocol is child stdin/stdout; a product
named-pipe command channel is not established by static evidence.

Supervision is unusually mature: PID file, executable-name validation through
`tasklist`, stale child cleanup, a ping/readiness gate, queued messages, critical
state replay, async callback IDs/timeouts, stale-event rejection, and at most
five exponential restart attempts (`index.js:9033-9215,9230-9470`). Critical
state includes auth URL/token, user identity, plan/flags, exclusions/content
filters, and observer state.

Windows BrowserWindows explicitly set `nodeIntegration: false` and
`contextIsolation: true` (`index.js:5197-5198,6139-6140,10291-10292`). The main
window denies new-window creation after opening HTTP(S) externally and blocks
navigation from sandboxed `srcdoc` frames (`index.js:10395-10445`). The preload
is large and covers many domains, so sender/window scoping still matters.
