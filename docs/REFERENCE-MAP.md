# Reference Map

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

- Native overlay with capture exclusion on macOS and Windows source.
- Command bar with model route and answer mode selection on macOS.
- Consent-based screen capture through support flows, plus overlay active-page capture for long browser content.
- File/context attachment and text/code preview extraction.
- Provider route contracts and OpenAI-compatible HTTP path for configured providers.
- Simulated dual audio/STT runtime that feeds the real transcript path.
- Cloud/RAG API, schema, queue, and deployment skeleton.

Still missing:

- Real native audio capture and streaming STT.
- Streaming answer rendering with citations.
- OCR/vision extraction for screenshots and documents.
- Settings/onboarding UI and meeting history dashboard.
- Windows parity and signed installers.
- Production cloud auth/sync/RAG/billing services.
