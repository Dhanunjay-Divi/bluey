# Electron main-process observations

Source: `LockedIn.app/Contents/Resources/app.asar:public/electron.js`, SHA-256 `2d5ecddd8d5a39883273f7adb6da137979da9ae0174c04a181c929cd5a5068e0`.

The file is readable bundled JavaScript. Line ranges refer to the extracted, byte-verified copy from the static phase. A later approved isolated launch is documented separately in `runtime-unauthenticated.md`.

| Lines | Observed fact |
|---|---|
| 1 | Loads `.env.local`. |
| 5-23 | Imports Electron APIs, `electron-updater`, logging, Firestore support, and child-process spawning. |
| 89-234 | Renderer crash, unresponsive, and load-failure recovery: at most three reloads per 60 seconds, a 15-second unresponsive grace period, then a manual recovery dialog. |
| 235-243 | Initializes Firestore in the main process. |
| 246-268 | Defines global shortcuts for screenshots, click-through, always-on-top, cursor window, microphone/system-audio mute, pausing, submitting the latest interviewer message, moving the window, and starting/ending a session. |
| 270-319 | Selects architecture-specific custom audio modules. |
| 321-350 | Checks screen-source access and applies an eight-second source lookup timeout. |
| 400-433 | App state includes current user ID, development flag, stealth enabled by default, window references, and platform. |
| 441-701 | Audio manager monitors capture; constants encode five-second checks, ten-second silence restart, three-second post-wake delay, thirty-second restart spacing, and three restart failures. |
| 794-809 | Applies macOS content protection according to stealth state. |
| 811-853 | Creates the main BrowserWindow with Node integration off, context isolation on, remote module off, background throttling off, and the preload; loads local `build/index.html#/app/dashboard`. No explicit `sandbox: true` appears, but the approved runtime process tree showed the renderer launched with `--enable-sandbox`; see `runtime-unauthenticated.md`. |
| 830-846 | Hides the window from the taskbar, keeps it always on top, and exposes it on all workspaces. |
| 903-930 | Closing/hiding keeps the app in the background instead of quitting. |
| 938-1033 | Creates cursor and document windows; the document window uses the same main session and preload. |
| 1285-1620 | Captures full-screen and cropped screenshots with desktopCapturer and sends base64 PNG data to the renderer. |
| 1624-1683 | Handles display-media source selection; Windows loopback audio is requested while macOS audio is false in this path. |
| 2139-2201 | Enforces a single instance, configures a background floating widget, debug file logging, and crash reporting. |
| 2203-2215 | Configures crashReporter with uploads disabled and an invalid submit URL. |
| 2218-2234 | Enables prerelease and downgrade updates, auto-download, install-on-quit, and run-after-install; logs process information including `argv`. |
| 2320-2463 | Registers updater events, checks after a two-second delay, and performs manual HEAD checks against latest YAML resources. |
| 2466-2510 | Parses the `locked-in:` deep link, recognizes Duo helper IDs, extracts `firebaseCustomToken` and `clerkUserId`, and sends them to the renderer. |
| 2512-2585 | Reads protocol URLs from process arguments, `open-url`, and second-instance events. |
| 2647-2669 | Toggles stealth/content-protection behavior. |
| 2694-2846 | Registers renderer logging, log reads, system info, user/shortcut/process state, navigation, and document-window IPC. |
| 2935-3102 | Receives normalized remote mouse, keyboard, and scroll messages over renderer IPC and invokes robotjs against the primary display. This proves an input-automation capability boundary, not that a user approved or used it. |
| 3104-3326 | Registers screenshot, media permission/status, screen-source, thumbnail, and audio IPC handlers. |
| 3328-3338 | Opens any nonempty renderer-supplied URL externally; no scheme allowlist is present in this handler. |
| 3340-3374 | Exposes update IPC. |
| 4079-4405 | Resolves/chmods/spawns the global-key helper and falls back to Electron global shortcuts; the packaged arm64 path does not statically skip the x86_64-only MacKeyServer. The audit did not exercise this spawn path. |
| 4560 | Instantiates the application. It was not executed during static analysis; the later approved isolated launch necessarily exercised ordinary initialization. |

Searches of the file found no `setPermissionRequestHandler`, `setWindowOpenHandler`, `will-navigate`, `safeStorage`, or Keychain usage, and no explicit renderer sandbox option. The separate approved runtime observation confirmed Electron nevertheless enabled its renderer sandbox.
