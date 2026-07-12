# Bluey Icon Guide

Bluey is meant to be usable from `bluey on` without memorizing terminal commands.

## Header

- Green dot: Bluey is running and the overlay is connected.
- Sparkles: focus the bottom ask box.
- Question mark: show the in-app control guide.
- Session: continue the current session or archive it and start clean.
- Model menu: choose the answer route.
- Mode menu: tune answers for general, code, system design, meeting, or writing.
- Opacity slider: make Bluey's background glass more transparent or more solid while text and icons remain readable.
- Paperclip: attach documents, code, screenshots, diagrams, or notes, or show what is already attached.
- Notepad: set answer rules for this session.
- Trash: clear visible overlay cards.
- Eye slash: collapse the overlay into a small Bluey button. Click the button to reopen.
- X: asks before quitting Bluey completely, the same as `bluey off`.

## Bottom Tray

- Mic: start or stop audio capture. Release `0.1.99` lists macOS Apple silicon and Windows x86-64 artifacts. Bluey uses bundled native helpers, applies VAD, routes through managed or local transcription paths, and deletes its temporary raw-audio chunk after transcription. Mock transcript cards are allowed only during explicit local smoke tests with `BLUEY_AUDIO_SIMULATED_ONLY=1` or `CUE_AUDIO_SIMULATED_ONLY=1`; normal user Listen must use real audio or show a setup error. FFmpeg remains a fallback/dev path.
- Mic dot: dim means off; bright green means recording.
- Ask field: ask using transcript, screen context, documents, page context, and memory.
- Send: submit the current question; if the field is empty, answer the latest clear question from the session context.
- Answer: answer the typed/current question, or the latest clear transcript/page/file question when the field is empty.
- Recap: summarize the session.
- Analyse Screen: search/read the active browser page or available screen context, attach it as context, and generate an answer.

## Cards

- System transcript rows: compact source-labeled text from system audio or page/meeting audio.
- Mic transcript rows: compact source-labeled text from the user's microphone.
- You cards: questions sent from the ask field or Answer button.
- Bluey / Response cards: streamed responses generated from the current transcript, page/screen context, attached files, and answer style.
- Context cards: files, screenshots, page captures, or memory.
- Action item cards: detected follow-ups.
- Decision cards: detected decisions.
- Warning cards: permission, provider, or capture issues.

The middle card area is readable but click-through, so the app behind Bluey stays usable. Wheel or trackpad scrolling over that area scrolls Bluey's cards without turning the whole overlay into a blocking window.
