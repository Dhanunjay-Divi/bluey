# Bluey Handoff

> Historical implementation handoff. Its `0.1.0` packaging statements are not
> current release claims. The public `0.1.101` manifest lists macOS Apple silicon
> and Windows x86-64 artifacts.

This document is the quick-start map for the next agent or engineer picking up Bluey.

## Product Shape

- Customer entrypoint is `bluey on`.
- Customer stop path is `bluey off` or the overlay X/Quit confirmation.
- CLI subcommands such as `bluey ask`, `bluey audio status`, `bluey providers`, and `bluey context add` are developer/support surfaces, not the main user flow.
- The overlay is the product surface: top command bar, scrollable question/answer/transcript feed, and bottom ask composer with Answer, Recap, and Analyse Screen.
- Capture remains consent/user-controlled. Do not add process disguise, monitoring bypass, or hidden capture behavior.

## Current Working Path

- `bluey on` starts a detached daemon and native overlay.
- Hidden/collapsed overlay stays hidden while passive cards arrive; the small Bluey pill restores it.
- Questions entered in the bottom composer create a question card, route to the answer runtime, then create an answer card.
- The overlay feed is chronological: `SYSTEM` and `MIC` transcript rows stream in as compact source-labeled cards, a submitted question appears as `YOU`, and the generated response appears next as `BLUEY`.
- Recent `YOU`/`BLUEY` Q&A turns are stored in the active session and included as `MeetingMemory` context for follow-up answers.
- Empty Answer/send asks Bluey to answer the latest clear question from transcript, page context, and attached files.
- Answers create a `BLUEY / Thinking...` card immediately. Live OpenAI-compatible providers stream deltas into that card through `OverlayCommand::UpdateCard`; local fallback and non-streaming paths replay the completed answer word by word.
- Provider context is compacted before every model call: transcript, recent Q&A, documents, screenshots, and notes are capped per item and then capped again as a total context block.
- Recap uses the daemon recap path.
- Analyse Screen keeps the overlay open, captures readable active browser page text when available, attaches it, then asks Bluey to answer from that context. If page text is unavailable, it captures one screenshot and routes it to a configured OpenAI-compatible vision provider. The overlay is excluded from normal screen capture.
- Attach accepts readable context only: text, Markdown, code, PDF, DOC, DOCX, and RTF. Unsupported/unreadable files are rejected instead of becoming fake context.
- Text/code/Markdown previews are local and ready immediately.
- PDF uses local `pdftotext` if installed.
- macOS DOC/DOCX/RTF uses `textutil`.
- Windows DOCX uses a PowerShell XML extraction path; legacy DOC/RTF needs the cloud parser.
- Native audio helper paths exist for macOS and Windows source builds. v0.1.0 ships the macOS arm64 path; without a configured remote/local STT path, the daemon can still fall back to dev-only mock/echo transcript flow for testing.
- v0.1.0 packaging is macOS arm64-only: `make package-darwin-arm64` creates a terminal tarball with CLI, daemon, overlay helper, audio helper, and whisper helper; `scripts/install.sh` installs it into a versioned local prefix.

## Keys And Local Models Needed For Real Testing

For real answer generation:

- `OPENAI_API_KEY` for OpenAI-compatible chat and STT testing.
- `GROQ_API_KEY` for low-latency chat route testing.
- `CEREBRAS_API_KEY` for fast chat route testing.
- Optional `ANTHROPIC_API_KEY` and `GOOGLE_API_KEY` for provider matrix testing.
- Optional Bluey-managed route: `BLUEY_CLOUD_API_URL` and `BLUEY_CLOUD_API_TOKEN`.
- Screenshot vision fallback: `OPENAI_API_KEY` works by default, or set `BLUEY_VISION_PROVIDER` and `BLUEY_VISION_MODEL` for an OpenAI-compatible vision route. Groq vision requires an explicit vision model.

For STT/audio:

- `DEEPGRAM_API_KEY` for Deepgram Nova-3 streaming STT.
- `OPENAI_API_KEY` or `BLUEY_STT_API_KEY` for OpenAI Realtime / compatible STT.
- `BLUEY_STT_LOCAL_WHISPER=1` plus a local whisper model for macOS LocalWhisper testing.
- Optional STT/provider env vars documented in `docs/work/PLAN-STT-FALLBACK-CHAIN.md`.

For local/offline model testing:

- Decide the local runtime: Ollama, llama.cpp server, or another OpenAI-compatible local endpoint.
- Provide the endpoint and model contract before wiring it as a first-class selector.
- Current local fallback is deterministic app logic, not a real local LLM.

For document/context testing:

- Install `pdftotext` locally for PDF parsing, or wire the cloud parser.
- Provide sample `.md`, `.txt`, code, `.pdf`, `.docx`, and intentionally unsupported files for regression checks.

## Verification Commands

Use these after meaningful changes:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
cd crates/cue-dashboard/ui && npm test && npm run build
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git diff --check
```

Product smoke:

```sh
./target/debug/bluey off
./target/debug/bluey on
./target/debug/bluey ask "Give me a one sentence status check."
./target/debug/bluey off
```

## Next Engineering Slices

- Clean-machine validate `scripts/install.sh` + `bluey on/off` on Apple Silicon.
- Replace local RAG linear cosine with sqlite-vec / ANN.
- Port real whisper.cpp to Windows and QA overlay/audio/page capture on Windows hardware.
- Add richer OCR/vision extraction with citations, thumbnail previews, queueing, and cloud processing status. The first screenshot-to-vision fallback path is wired for Analyse Screen.
- Add cloud auth/device registration, artifact upload, document parsing, embeddings, and tenant-scoped RAG.
- Add settings/onboarding UI for account, permissions, audio devices, models, hotkeys, retention, export, and deletion.
- Add a history/dashboard UI for sessions, transcripts, recaps, answers, and attachments.
- Add signed macOS/Windows installers, auto-update, telemetry opt-in, and support diagnostics when ready for public distribution.

## Review Notes

- Keep `docs/WORKLOG.md` updated by appending a new round for each product slice.
- Update `docs/SELF-REVIEW.md` when a risk becomes fixed or a new risk appears.
- Avoid claiming a feature is production-ready until it has a real runtime path and has been tested on the target OS.
