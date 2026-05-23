# Bluey

Bluey is a lightweight native AI work copilot. This repo is starting from a clean Rust/native foundation, using the reference packages in this folder as architectural inspiration.

Product direction: Bluey is a commercial managed-cloud product, not an open-source/BYOK clone. The current v0.1.0 track is a macOS arm64, local-first terminal distribution with a native overlay and bundled helpers. Managed cloud sync, managed provider routing, cloud RAG, billing, and workspace controls remain the paid-product path.

Current shape:

- `bluey` CLI: terminal-first lifecycle (`bluey on` / `bluey off`) plus
  hidden support/admin commands for diagnostics and automation.
- `bluey-daemon`: local background process and IPC server.
- `cue-core`: shared protocol, cards, state, paths.
- `native/macos/cue-overlay`: AppKit overlay sidecar, excluded from screen capture with `NSWindow.sharingType = .none`; compact pill-first startup, click-to-open feed/composer, tokenized IPC, attach/instructions/recap/ask events, and opacity controls.
- `native/windows/cue-overlay`: Win32 overlay sidecar source with Direct2D rendering and capture-exclusion work, kept for the Windows parity round. Windows is not shipped in v0.1.0 until hardware QA and Windows whisper.cpp are complete.

## Build

```bash
bash native/macos/cue-overlay/build.sh
cargo build
```

Terminal-first release folders:

```bash
scripts/build-macos.sh
```

On Windows with Rust and MSVC Build Tools:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

## Smoke Test

For an isolated repeatable run:

```bash
scripts/smoke-test.sh
```

That command is only a health check. To actually use Bluey locally:

```bash
./target/debug/bluey on --title "My test meeting"
```

To turn Bluey off:

```bash
./target/debug/bluey off
```

There is no separate customer login command. If Bluey has no stored account
token, `bluey on` opens the browser sign-in flow and then continues the same
session after the app receives the `bluey://` callback. Support/dev commands
remain hidden for diagnostics and local test automation.

That opens the overlay-first flow. Use the overlay buttons for normal setup:

```text
sparkles: focus the ask tray at the bottom
question mark: explain every icon and dot
session: continue the active session or start a clean one
bottom mic: start/stop recording; green dot means recording is active
bottom send: ask Bluey from the current screen, transcript, files, and memory
analyse: read the active browser page and generate an answer
recap: summarize the active session
paperclip: attach files or show attached context for this session
notepad: set answer style/rules
opacity slider: adjust background glass transparency while keeping text and icons readable
```

The overlay can be moved by dragging its top bar. Resize it from the edges or bottom-right grip. Use the header slider or `/opacity 70` to adjust background glass transparency. Text and icons stay readable because the slider no longer fades the whole macOS window. The macOS overlay remembers its last frame and opacity between runs.
The header and bottom tray are interactive. The middle reading area is click-through so the app underneath Bluey remains usable, while wheel/trackpad scrolling over the feed still scrolls Bluey's history.
Use the Session icon to continue the active session with its existing transcript/context, or start a clean session. A new session archives the current one first.
Use the Analyse chip when a browser page has more text than is visible on screen. Bluey asks supported browsers for readable page text, attaches it as session context, and generates an answer; macOS may show the normal Automation permission prompt, and Windows uses user-level UI Automation when browsers expose document text. If page text is unavailable, Bluey can fall back to one permissioned screenshot plus an OpenAI-compatible vision route when `OPENAI_API_KEY`, `BLUEY_VISION_PROVIDER`, or Bluey managed vision is configured.
Use the paperclip icon to select session files or show what is already attached. Use the notepad icon to set how Bluey should answer questions. Use the eye-slash icon to collapse the overlay into a small Bluey button; click that button to reopen. Use X/Quit when you want to stop Bluey completely.

Real audio transcription is wired through bundled native helpers and the streaming STT provider chain. On macOS, Bluey uses native system/mic capture helpers, two-stage VAD, Deepgram/OpenAI Realtime/LocalWhisper-capable STT routing, and source-labeled transcript storage. Windows audio helper source exists but is not part of the v0.1.0 support matrix until Windows QA is complete. Users do not need loopback drivers for the primary macOS path. FFmpeg remains a fallback/dev path, with optional `BLUEY_FFMPEG_PATH`, `BLUEY_MIC_AUDIO_DEVICE`, and `BLUEY_SYSTEM_AUDIO_DEVICE` overrides.

The live feed is chronological and scrollable: system audio appears as `System transcript`, microphone audio appears as `Mic transcript`, submitted questions appear as `You`, and generated responses appear as `Bluey / Response` cards directly after the question. Recent turns are stored in the active session so follow-up questions continue from the previous answer.

Bluey streams answers into the same response card: live OpenAI-compatible providers update the card as deltas arrive, while local fallback/non-streaming answers replay word by word. Coding answers render fenced code as a dedicated code pane beside the explanation when possible. Before each model call, Bluey compacts transcript, recent Q&A, documents, screenshots, and notes so the request stays inside the active model window.

Development and support commands remain hidden from normal help. The visible
terminal product surface is only `bluey on` and `bluey off`; account, billing,
history, and settings live in the overlay/dashboard.

The daemon listens on `127.0.0.1:57321` by default. Set `BLUEY_DAEMON_ADDR`, `BLUEY_DAEMON_BIN`, or `BLUEY_OVERLAY_BIN` to override local development paths. Set `BLUEY_DATA_DIR`, `BLUEY_CONFIG_DIR`, or `BLUEY_RUNTIME_DIR` to isolate local state during testing. The older `CUE_*` names still work as compatibility aliases.

## Direction

The hot path stays small:

```text
audio chunk -> VAD -> STT partial/final -> rolling context -> LLM stream -> overlay card
```

No Electron and no Python runtime in the first native build. Saved sessions and settings are terminal-first now; a fuller dashboard can come later if it earns its keep.

## First Testable Version

This build is testable with or without provider keys. The customer-visible loop is:

- `bluey on --title "Name"` starts the overlay-first session and opens sign-in
  only when needed.
- Use the larger bottom mic button to start/stop real chunked audio transcription when STT credentials are present; otherwise Bluey falls back to the labeled development simulator for local testing.
- Use the bottom ask tray and send button to ask Bluey questions.
- Use Analyse, Recap, paperclip, notepad, model picker, and mode picker from the overlay.
- Use the overlay/dashboard for saved session history, answer style, settings,
  balance, and account actions.
- `bluey off` stops Bluey.

The audio path feeds the same session engine either way: source-labeled transcript segment in, context-aware answer cards out.

See `docs/FEATURE-MAP.md` for the ethical translation of the reference-app feature checklist into Bluey's roadmap.
See `docs/WORKLOG.md` for the round-by-round implementation history.
See `docs/ICON-GUIDE.md` for every overlay icon, dot, and card type.
See `docs/SESSION-FLOW.md` for the overlay-first user flow.
See `docs/PRODUCT-STRATEGY.md` and `docs/CLOUD-RAG.md` for commercial cloud/RAG direction.
See `docs/COMPETITIVE-GAPS.md` for the paid-product gap list.
See `docs/PRODUCTION-READINESS.md` for the current implemented/missing production matrix.
See `docs/PRE-PRICING-REVIEW.md` for the working checklist before pricing plans.
See `docs/IMPLEMENTATION-SEAMS.md` for the code seams that native audio, STT, managed AI, and cloud RAG should plug into next.
See `docs/DEPLOYMENT-SCALING.md` for packaging, cloud APIs, storage, provider keys, and scaling plan.
See `docs/BACKEND-CONTRACTS.md`, `docs/SETTINGS-UI-CONTRACT.md`, `docs/INSTALLER-CHECKLIST.md`, and `infra/` for the production backend/deployment skeleton.
