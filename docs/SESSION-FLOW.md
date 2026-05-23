# Session Flow

Bluey should feel like an overlay-first, terminal-first product. `bluey on`
and `bluey off` are the customer-facing live-session commands. The first
`bluey on` opens browser sign-in when needed; the overlay/dashboard handles
account, history, answer style, and settings from there.

## Normal User Flow

Start Bluey:

```bash
bluey on --title "Session name"
```

Bluey first appears as a compact pill. Click the pill or use the show/toggle
path to open the full overlay with a persistent ask tray at the bottom.

The header, resize edges, and bottom ask tray accept clicks. The middle feed remains click-through so the app behind Bluey stays usable, but wheel/trackpad scrolling over the feed scrolls Bluey's transcript and answer history.

Use the bottom tray for the main loop:

- Mic: start/stop recording. The green dot turns bright while recording is active.
- Ask field: type a question for the current screen, transcript, files, or memory.
- Send: submit the question without leaving the overlay.
- Quick chips: answer, recap, or analyse the active browser page.

Audio/STT text appears as source-labeled transcript cards: `System transcript`
for system/browser audio and `Mic transcript` for microphone audio. Typed
questions render as `You` cards, and model output renders as `Bluey / Response`
cards. On the v0.1.0 macOS path, Bluey captures audio through bundled native
helpers, applies VAD, and routes transcription through configured Deepgram,
OpenAI Realtime, or LocalWhisper providers. Windows WASAPI source exists for the
parity round but is not shipped in v0.1.0 until Windows whisper.cpp and hardware
QA are complete. FFmpeg and mock/echo providers remain fallback/dev paths.

Use the header icons for setup and window control:

- Sparkles: focus the ask tray at the bottom of Bluey.
- Question mark: explain every icon, dot, and card type.
- Session: continue the active session or archive it and start clean.
- Paperclip: choose PDFs, docs, screenshots, diagrams, code, or text files for this session, or show what is already attached.
- Notepad: set how Bluey should answer questions in this session.
- Slider: adjust background glass opacity while text and icons remain readable.
- Trash: clear visible cards.
- Eye slash: collapse the overlay into a small Bluey button. Click the button to reopen.
- X: asks before quitting Bluey completely, the same as `bluey off`.

Stop Bluey:

```bash
bluey off
```

Support/dev terminal surfaces still exist behind the scenes for diagnostics,
automation, and smoke tests, but they are not part of the paid customer flow.

## Answer Instructions

Use the notepad icon for this. Examples:

```text
Answer briefly. Prefer implementation steps. Mention risks. Do not guess.
```

```text
Use a senior engineering tone. Give concise tradeoffs and next actions.
```

Bluey stores this on the active meeting and includes it in answers/recaps.

## Context

Context belongs to the current session. Continuing a session keeps its transcript, answer rules, attached files, page captures, and screenshots. Starting a new session archives the current one and begins with clean context.

Use the paperclip icon first. It can open a native file picker and attach selected files to the active meeting, or show the documents, screenshots, page captures, and notes already attached.

Use the Analyse Screen quick chip when the important content is a browser page that extends beyond the visible viewport. After confirmation, Bluey first asks a supported browser for readable page text, saves it as session context, and generates an answer. If browser text is unavailable, Bluey falls back to one screenshot and sends it to a configured vision route. The overlay stays open and is excluded from normal screen capture. On macOS, browser text works for supported Chrome-family browsers and Safari through normal browser scripting permissions; screenshot fallback uses the platform screenshot path. On Windows, browser text uses user-level UI Automation where the browser exposes document text, then falls back to the Windows screenshot path when a vision route is available.

Use terminal fallback capture commands when the useful context is not available as browser text.

## Terminal Fallbacks

These remain hidden from normal help for testing, automation, diagnostics, and support:

```bash
bluey listen --speaker system "..."
bluey ask "what are the action items?"
bluey context add ./file.pdf --note "Use this as architecture reference"
bluey context page
bluey context capture --title "Architecture diagram" --note "whiteboard from planning"
bluey instructions set "Answer briefly and focus on implementation risks."
bluey instructions show
bluey instructions clear
bluey audio status
```

The product direction is to keep daily work in the overlay and keep terminal commands small enough to remember.

Internal-only command groups include meeting lifecycle commands, `bluey run`, `bluey listen`, `bluey ask`, `bluey recap`, `bluey memory search`, `bluey audio ...`, `bluey ai status`, `bluey cloud ...`, and `bluey providers`. They are useful for building and verifying the product, but paid customers should be able to complete setup and daily use without them.
