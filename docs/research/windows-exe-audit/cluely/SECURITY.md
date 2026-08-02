# Cluely 2.0.193 Windows security assessment

## Positive observations

- Installer and both main executables pass offline Authenticode digest and chain
  verification to Cluely Inc.
- Cross-origin navigation/redirects are denied and new windows only open
  HTTPS/mailto externally (`dist-electron/main.js` offsets 494222-494900).
- Main IPC handlers check the sending renderer origin.
- Shared-state writes use temporary-file replacement.
- Content protection is applied to overlay/dashboard/notification windows.

## Risks

- The 314-byte preload exposes generic channel `send`/`invoke`/`on` rather than a
  capability-specific API.
- BrowserWindow flags do not explicitly set `contextIsolation`, `nodeIntegration`,
  or `sandbox`; Electron defaults are an inference, and renderer sandboxing is
  not established.
- The display-media handler picks the first screen and grants loopback without an
  observed request-origin check. Renderer compromise would reach a powerful
  capture path.
- Authentication lives in Chromium state; no application `safeStorage` or
  Credential Manager boundary was found.
- The updater manifest names a feed, but no independent publisher pin or signed
  manifest check is visible in Cluely code.
- No bounded native-helper integrity manifest is checked before SoX launch in
  application code. The supplied SoX is signed, but replacement behavior after
  installation was not validated.

Bluey should adapt the simple Chromium-loopback fallback only as a compatibility
path. Its primary helper should remain owner-checked, hash/publisher verified,
bounded, and independent of renderer trust.
