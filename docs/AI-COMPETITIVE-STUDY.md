# Bluey AI Competitive Study

This note summarizes the safe engineering lessons from the local reference packages reviewed during Round 57. It intentionally focuses on consent-based input, answer streaming, context quality, and overlay UX. It does not adopt deceptive positioning or proctoring/monitoring bypass behavior.

## References Reviewed

- `/Users/uno/Downloads/OpenCluely-main`
- `/Users/uno/Downloads/cue/pluely-master.zip`
- `/Users/uno/Downloads/cue/natively-cluely-ai-assistant-main.zip`
- `/Users/uno/Downloads/cue/Aura-AI-master.zip`
- `/Users/uno/Downloads/pinky-git`
- Bluey code in `/Users/uno/Downloads/cue`

## Current Bluey Strengths

- Native daemon + overlay architecture keeps the UI lightweight and terminal-first.
- OpenAI-compatible providers already use real SSE token streaming into the overlay.
- The session model feeds transcript, attached context, recent Q&A, and notes into provider prompts.
- The overlay renders streamed answers as readable cards and turns fenced code blocks into code panes.
- The macOS overlay is click-through in the reading area while header/composer/resize controls stay interactive.

## Gaps Found

- Bluey Auto from the overlay was using a direct managed route, so it could fail instead of falling back through Groq/Cerebras/OpenAI/local.
- Direct provider labels used managed proxy model names such as `managed-reasoning`, which are not valid raw API model IDs.
- Selected answer mode instructions were overriding notepad/session answer rules instead of combining with them.
- General mode needed an explicit product rule: auto-detect coding questions and still use code-shaped output.
- The daemon returns structured `AnswerStreamEvent`s only after completion; the overlay streams live, but IPC clients do not yet get true incremental events.
- Answer generation now has stale-update protection, but true upstream request cancellation is still pending.
- Context compaction is character-budget based; it needs relevance and recency ranking for production-scale RAG.
- Windows overlay parity still lags the macOS overlay for scroll history, model/mode controls, and robust JSON handling.

## Source-Level Patterns Worth Keeping

- Natively's `IntelligenceEngine` uses generation ids and cancellation tokens so old responses cannot overwrite newer answers. Bluey now has the same stale-update guard in the daemon; the next step is provider-level abort.
- Natively's `SessionTracker` keeps interim transcript separate from final transcript, dedupes final transcript retries, stores assistant history, and compacts older context. Bluey now dedupes recent final transcript retries and already includes recent Q&A in answer context.
- Pluely's chat hook assigns a request id, aborts the previous controller, updates the visible assistant message while tokens arrive, and ignores chunks for stale request ids. Bluey's overlay streams into one answer card and now ignores stale generations.
- Aura's live UI preserves auto-scroll only while the user is at the tail, and pauses it when the user scrolls up to read. Bluey's macOS feed now follows that behavior with a small `Latest` affordance.
- Pinky's native caption overlay keeps capture exclusion, click-through reading text, pointer-over wheel scrolling, and movable/resizable native chrome. Bluey keeps that interaction split for the feed, header, composer, and resize edge.

## Improvements Applied In Round 57

- `Bluey Auto` now maps to the managed commercial route with fallbacks.
- Managed fallback model IDs now use real direct-provider defaults instead of `managed-*` aliases.
- Overlay direct provider selections now send valid model choices or allow the daemon to apply provider defaults.
- General mode now explicitly auto-detects code/debug/API/config questions and keeps `Approach`, `Code`, `Explanation`, `Complexity`, and `Edge cases` with fenced code blocks.
- Session answer rules now merge with mode/request instructions.
- Tests cover Auto routing, direct provider alias cleanup, General coding shape, and instruction merging.

## Improvements Applied In Round 58

- Added daemon answer generations so a newer ask supersedes older overlay updates and prevents stale answers from being saved into session Q&A.
- Added a visible superseded state for the prior answer card when a newer answer starts.
- Added final-transcript dedupe for recent STT retries so repeated system/mic chunks do not pollute the model context.
- Added macOS feed tail-follow behavior: Bluey streams to the latest card while the user is at the bottom, pauses auto-scroll while the user scrolls up, and shows a `Latest` button to jump back.
- Clarified the local fallback response so no-key testing is honest about context-only output versus a generated provider answer.

## Next Product-Quality AI Work

- Stream `AnswerStreamEvent` frames over IPC for CLI/dashboard clients, not only overlay cards.
- Add provider-level abort/cancellation so superseded HTTP requests stop consuming provider latency and tokens.
- Add provider-specific adapters for Anthropic, Google, and future multimodal APIs instead of only OpenAI-compatible chat completions.
- Add ranked context selection: recent transcript, recent Bluey answers, attached docs, page/screenshot context, then long-term memory.
- Add partial STT rows with final/partial replacement semantics so live audio feels continuous without polluting prompt context.
- Add Windows overlay history parity and smoke tests for pass-through clicks, scroll, resize, hide/collapse, and capture exclusion.
