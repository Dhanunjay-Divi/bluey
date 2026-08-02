# LockedIn 1.8.8 Windows security assessment

## Positive observations

- Installer, main executable, and WinKeyServer pass Authenticode digest/chain
  verification to Cyber Gravity LLC.
- BrowserWindows explicitly disable node integration and enable isolation.
- Content protection is repeatedly applied to private windows on Windows.
- System audio capture uses the selected source rather than silently selecting
  an arbitrary display.
- Screenshot capture has timeout/rate/busy guards and empty-image handling.

## High-impact risks

- The preload exposes unrestricted channel-level `send`, `on`, and `invoke` in
  addition to privileged log, URL, screenshot, window, process-name, update, and
  remote-input operations (`public/preload.js:127-250`).
- `execute-remote-input` accepts renderer messages and injects OS mouse/keyboard
  events without an observed origin/window/session capability check
  (`public/electron.js:2936-3110`). A renderer compromise becomes desktop input
  compromise.
- `navigate-to-url`, `open-external`, `show-item-in-folder`, and process-name
  controls need strict allowlists; the bridge itself does not constrain channels.
- The high-privilege robotjs `.node` is unsigned. Enclosing ASAR/app signing does
  not replace per-helper verification after installation.
- Packaged source/config/build scripts increase attack/reconnaissance surface and
  can expose environment assumptions.
- No app-level secure secret store was found.

Bluey should explicitly reject general remote-input IPC. Its existing challenge
takeover keeps the user inside a scoped browser page
(`jobs/automation/src/challenge-handling.ts:55-84`) and its daemon uses typed,
authenticated, replay-protected commands.
