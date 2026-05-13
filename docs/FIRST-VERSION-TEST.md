# First Version Test

This test does not require API keys or audio permissions. It verifies the core Bluey loop:

```text
CLI -> daemon -> meeting engine -> native private overlay -> persisted recap
```

## Build

```bash
bash native/macos/cue-overlay/build.sh
bash native/macos/cue-audio/build.sh
cargo build
```

## Run

Use Bluey interactively:

```bash
./target/debug/bluey on --title "Bluey real test"
```

The overlay should appear with the boot animation. Use the overlay controls first:

```text
bottom mic: start/stop recording
bottom ask tray: type and send questions
analyse: read the active browser page and generate an answer
paperclip: attach files or show attached context
notepad: answer style/rules
slider: opacity
```

Drag the overlay by its top bar. Resize from the edges or bottom-right grip. Adjust opacity with the header slider, `/opacity 70`, or `bluey overlay opacity 70`. On macOS, the frame and opacity are saved and restored on the next daemon start.
Click the Analyse Screen chip to read the active browser page or available screen context and generate an answer. This is the flow for long pages where only part of the question is visible without scrolling.
Click the paperclip icon to select session files. Click the notepad icon to set answer style/instructions for the session. Click the eye-slash control to collapse Bluey into a small button, then click that button to reopen. Use X/Quit or `bluey off` to stop it.

Use the scripted smoke test only as a clean pass/fail health check:

```bash
scripts/smoke-test.sh
```

Or run the same flow manually:

```bash
./target/debug/bluey on --title "Bluey test meeting"
./target/debug/bluey listen --speaker system "What is the plan for Bluey?"
./target/debug/bluey listen --speaker user "Action item: I will test the native overlay on Zoom."
./target/debug/bluey listen --speaker system "We decided to keep the first build Rust native and avoid Electron."
./target/debug/bluey ask "what are the action items?"
./target/debug/bluey action-items
./target/debug/bluey recap
./target/debug/bluey meeting end
./target/debug/bluey stop
```

Expected behavior:

- The macOS overlay appears when cards are pushed.
- `bluey on` starts/reuses a session, shows the overlay, and renders the boot card.
- The overlay stays excluded from screen capture through the native privacy API.
- Opacity can be adjusted from the overlay header or CLI.
- `context add` stores user-selected screenshots, diagrams, documents, and code files in the meeting record.
- `context capture` uses the OS capture picker on macOS, opens a preview, and asks before attaching.
- The Analyse Screen chip attaches active browser page text and generates an answer after confirmation. macOS uses browser scripting; Windows uses UI Automation where the browser exposes document text.
- The paperclip and answer-style overlay buttons reduce terminal-only setup.
- The bottom ask tray sends questions and renders answers as overlay cards.
- `audio status/start/stop` exposes the dual system/microphone runtime. With `OPENAI_API_KEY` or `BLUEY_STT_API_KEY`, Bluey uses bundled native helpers to capture short chunks and transcribe them: ScreenCaptureKit/CoreAudio on macOS and WASAPI on Windows. Without credentials it uses the labeled development simulator. FFmpeg remains a fallback/dev path.
- `ai status` exposes managed routing, fallbacks, and provider readiness.
- `cloud status` exposes secure sync/RAG readiness.
- `status` shows an active meeting while it is running.
- `action-items` returns the detected follow-up.
- `recap` includes transcript count, decisions, and action items.
- `meeting end` archives the record locally.

For isolated manual runs, set `BLUEY_DATA_DIR`, `BLUEY_CONFIG_DIR`, `BLUEY_RUNTIME_DIR`, and optionally `BLUEY_DAEMON_ADDR` before `bluey start`.

## Current Limits

- Audio capture has working native helper code for macOS ScreenCaptureKit/CoreAudio chunks and Windows WASAPI chunks. Windows still needs hardware QA on an actual Windows machine.
- STT is chunked near-real-time through an OpenAI-compatible transcription endpoint. True partial-token streaming, VAD, reconnects, and provider failover are still next.
- LLM routing can call OpenAI, Groq, Cerebras, or an OpenAI-compatible Bluey managed endpoint when keys are configured, and falls back to local deterministic answers when they are not.
- Cloud/RAG has sync and retrieval models, but authenticated upload/vector search is not wired yet.
- Windows overlay source is present and cross-compiled from macOS, but must still be QA-tested on Windows hardware. Windows page-text extraction uses UI Automation and should fall back to screenshot/file attach when a browser does not expose text.

The next production slice is hardening the audio/STT loop: VAD, partial transcripts, provider fallback, Windows hardware QA, and a settings UI for selecting devices and permissions.
