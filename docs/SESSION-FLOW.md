# Session Flow

Bluey is an overlay-first product with a terminal launcher. `bluey on` and
`bluey off` handle lifecycle; normal work happens in the overlay. The first
`bluey on` opens browser sign-in when needed, while the overlay/dashboard owns
account, history, answer style, data controls, and settings.

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
cards. The signed live manifest and current release note define supported
artifacts. Bluey captures audio through bundled native helpers, applies VAD,
and routes transcription through configured managed or local paths. Windows
release proof is on Windows 11; unsupported operating systems are not implied.
FFmpeg and mock/echo providers remain fallback/dev paths.

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

## Data Choices

- Sign-in enables managed answers and account balance. Cloud session sync is a
  separate Settings choice and defaults off for new installs.
- Legacy sync state without an explicit consent marker is treated as off.
- Raw audio is processed transiently for transcription and is not kept in a
  Bluey raw-audio library. Transcript text can remain in the local session.
- Submitted session content is not used for model training.
- Supported desktop paths request capture exclusion for the overlay, but this
  is best effort. Users should test the meeting app or capture path before
  relying on it.

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

Use the Analyse Screen quick chip when the important content is a browser page that extends beyond the visible viewport. After confirmation, Bluey first asks a supported browser for readable page text, saves it as session context, and generates an answer. If browser text is unavailable, Bluey falls back to one screenshot and sends it to a configured vision route. The overlay remains visible to the user and requests capture exclusion where the operating system supports it; that exclusion is not guaranteed. On macOS, browser text works for supported Chrome-family browsers and Safari through normal browser scripting permissions; screenshot fallback uses the platform screenshot path. On Windows, browser text uses user-level UI Automation where the browser exposes document text, then falls back to the Windows screenshot path when a vision route is available.

Use terminal fallback capture commands when the useful context is not available as browser text.

## Account Switching And Local Session Migration

Cloud session history is account-scoped. If a user signs out, deletes an
account, or links the desktop to a different account, old cloud chats should
stay with the old account. The new account should not see the old account's
history unless the user explicitly chooses to migrate local sessions.

Current policy:

- Moving a desktop to a new account changes the desktop link and billing
  identity only. It does not automatically move chats.
- Local cached sessions should be tagged with the account that created or
  synced them.
- On logout/account switch/delete, Bluey should stop listening and answering,
  clear visible context, hide old history, and reload only sessions owned by
  the current account.
- Migration is explicit and CLI-only for now:

```bash
bluey sessions --move-local-to-current-account --confirm-move-local-sessions
```

Future revisit:

- Make session IDs visible and searchable in overlay History and the web
  Session History page.
- Consider an overlay/web migration prompt only if it is safer and clearer than
  the CLI command.
- If UI migration is added, require an explicit checkbox and a typed
  confirmation. The copy should explain that selected local sessions will become
  visible in the current cloud account.
- Never migrate sessions automatically during login, logout, account deletion,
  or desktop relink.

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
