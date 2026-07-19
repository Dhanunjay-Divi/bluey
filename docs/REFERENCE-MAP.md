# Reference Map

> Historical reference snapshot. Current distribution facts come from the
> signed live manifest and `INSTALL.md`.

These packages remain references, not the Bluey foundation.

| Reference | What Bluey borrows |
|---|---|
| Pinky | Direct native overlay model, JSONL child process pattern, macOS/Windows capture exclusion details |
| Natively | Meeting lifecycle, dual-channel audio architecture, STT provider shape, session tracker, SQLite/RAG concepts |
| Pluely | Lightweight Rust/Tauri direction, global shortcuts, compact system-audio ideas |
| SolveWatch | Streaming STT/VAD flow, rolling partials, fast answer pipeline |
| Aura-AI | Windows Win32 overlay details and provider/key rotation ideas |
| OpenCluely/Vysper | Hotkey UX, prompt modes, transparency controls, screenshot flow |

Bluey goal:

```text
invisible + tiny + instant startup + meeting-grade audio + streaming answers
```

## Current Bluey Status Against References

Implemented or scaffolded:

- Native pill-first overlay with capture exclusion on macOS and Windows source.
- Command bar/composer with model route and answer mode selection on macOS.
- Consent-based screen capture through support flows, plus overlay active-page capture for long browser content.
- File/context attachment and text/code preview extraction.
- Native macOS audio helper, two-stage VAD, Deepgram/OpenAI Realtime/LocalWhisper-capable STT routing, and source-labeled transcript storage.
- Provider route contracts, streaming answer cards, OpenAI/Anthropic/Ollama/OpenAI-compatible adapters, and deterministic local fallback when explicitly selected.
- Local SQLite session storage, FTS search, export, local RAG primitives, and cloud/RAG API/schema/queue/deployment skeletons.
- macOS arm64 terminal package and installer script.

Still missing:

- Clean-machine macOS arm64 install validation.
- Citations and richer answer metadata on streamed responses.
- Production OCR/vision extraction for screenshots and documents.
- Settings/onboarding UI and meeting history dashboard.
- Windows real whisper.cpp, Windows hardware QA, and broader platform artifacts.
- Signed installers and auto-update after the platform matrix is real.
- Production cloud auth/sync/RAG/billing services.
