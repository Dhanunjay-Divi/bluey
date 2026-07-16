# Bluey Handoff

> Current implementation handoff for `0.1.102`. Release acceptance and runtime
> boundaries are recorded in
> `docs/rounds/ROUND-527-CONTEXT-RECOVERY-UX-AND-ATOMIC-RELEASE.md`.

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
- Context Watch is explicit and consent-first. It observes the foreground
  supported browser, prefers semantic page text, strips URL query/fragment
  data, deduplicates unchanged content, and stores only a bounded owner-private
  local history. Screenshot fallback is off by default.
- Meeting detection observes supported local process/audio evidence and offers
  Start, Ignore, and Settings. It does not start recording by detection alone.
- Answers create a `BLUEY / Thinking...` card immediately. Live OpenAI-compatible providers stream deltas into that card through `OverlayCommand::UpdateCard`; local fallback and non-streaming paths replay the completed answer word by word.
- Provider context is compacted before every model call: transcript, recent Q&A, documents, screenshots, and notes are capped per item and then capped again as a total context block.
- Recap uses the daemon recap path.
- Analyse Screen keeps the overlay open, captures readable active browser page text when available, attaches it, then asks Bluey to answer from that context. If page text is unavailable, it captures one screenshot and routes it to a configured OpenAI-compatible vision provider. The overlay is excluded from normal screen capture.
- Attach accepts readable context only: text, Markdown, code, PDF, DOC, DOCX, and RTF. Unsupported/unreadable files are rejected instead of becoming fake context.
- Text/code/Markdown previews are local and ready immediately.
- PDF uses local `pdftotext` if installed.
- macOS DOC/DOCX/RTF uses `textutil`.
- Windows DOCX uses a PowerShell XML extraction path; legacy DOC/RTF needs the cloud parser.
- macOS packages contain the Rust CLI/daemon plus native overlay, dual-audio,
  file-picker, and real local-Whisper helpers. Apple silicon, Intel, and
  universal archives are built and verified separately.
- Windows x86-64 packages contain the Rust CLI/daemon plus native overlay,
  dual-audio, and capture helpers. Managed live captions are the supported STT
  path; no placeholder or fabricated local transcript helper is shipped.
- Jobs has a separate Web portal and local/cloud runner architecture for
  discovery, ranking, factual resume tailoring, answer memory, final review,
  ATS-specific execution, handoff-only sites, receipts, interview preparation,
  and encrypted crash recovery.

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

## Remaining Runtime And Dependency Gates

- Run physical Windows launch, overlay, dual-audio, UI Automation context,
  update, DPAPI checkpoint, and managed-live-caption canaries before broad
  Windows rollout.
- Run macOS Intel and clean-machine universal install/update canaries.
- Validate Chrome/Edge semantic capture under customer browser permission
  policies; unsupported policies fail closed instead of silently taking
  screenshots.
- Jobs mailbox/calendar OAuth, outcome ingestion, and provider certification
  remain disabled until provider credentials, review, and live test accounts
  are available.
- Measure tenant-filtered vector retrieval before replacing the current bounded
  retrieval strategy with a production ANN index.

## Review Notes

- Keep `docs/WORKLOG.md` updated by appending a new round for each product slice.
- Update `docs/SELF-REVIEW.md` when a risk becomes fixed or a new risk appears.
- Avoid claiming a feature is production-ready until it has a real runtime path and has been tested on the target OS.
