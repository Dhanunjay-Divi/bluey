# Preload and IPC observations

Source: `LockedIn.app/Contents/Resources/app.asar:public/preload.js`, SHA-256 `043a7dcf1ff58b9fe3726e7e5f542c911694d52b2646b20ba3c56885eb76ce93`.

| Lines | Observed fact |
|---|---|
| 1-5 | Uses Electron `contextBridge` to expose APIs to the renderer. |
| 5-123 | Exposes named capabilities for microphone/system audio, display capture, screenshots, permissions, shortcuts, updater state, authentication events, logging, and navigation. |
| 124-140 | Adds generic `send(channel, ...)`, `on(channel, ...)`, `invoke(channel, ...)`, and `removeAllListeners(channel)` wrappers without a preload-side channel allowlist. |
| 182-245 | Exposes stealth, click-through/transparency, navigation, document windows, process-name changes, mouse-ignore state, and `executeRemoteInput`. |

Observed boundary: Node integration is disabled and context isolation is enabled in the BrowserWindow, but the generic wrappers make the effective renderer-to-main authority broader than the named API suggests. Runtime exploitability depends on renderer compromise and main-handler validation; sender/origin validation was not observed in the corresponding main handlers.
