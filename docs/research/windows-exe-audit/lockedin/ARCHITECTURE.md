# LockedIn 1.8.8 Windows architecture

## Observed system

```text
Mic MediaStream + Chromium display-media loopback
screenshots/documents ──► React renderer
                              │ broad preload IPC
                              ▼
Electron main ─ windows/widgets/global shortcuts
     ├─ WinKeyServer global keyboard events
     ├─ robotjs remote mouse/keyboard injection
     └─ Firebase/Clerk + Socket.IO/WebRTC backend
```

`public/electron.js` and `public/preload.js` are readable. BrowserWindows set
`nodeIntegration: false` and `contextIsolation: true` but do not explicitly
enable renderer sandboxing (`electron.js:817-818,953-954,991-992,1771-1772`).
The preload exposes typed convenience calls and also unrestricted `send`, `on`,
and `invoke` wrappers (`preload.js:1-250`).

Windows display-media handling uses a previously selected screen and returns
`audio: "loopback"` (`electron.js:1620-1682`). The custom `AudioManager` and its
10-second silence watchdog/30-second retry gap/three-failure budget are
macOS-only (`electron.js:270-319,440-704`). Windows microphone handling is in the
renderer/browser media path.

The most sensitive boundary is helper collaboration. WebRTC data-channel input
is forwarded by the renderer as `execute-remote-input`; the main process loads
robotjs and injects mouse moves/click/drag, keys, and scrolling, including Windows
DPI scaling (`electron.js:2936-3110`; `preload.js:245`). No sender-frame or
session-capability check is visible in the handler.

Screenshots support full display and bounded-area capture. Content protection is
reapplied to several windows, including after Windows hide/show. Session state,
presets, documents, and Socket.IO/WebRTC collaboration live primarily in the
React renderer/backend.
