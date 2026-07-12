# Self Review

> Historical self-review of the original macOS-first build. It is not a current
> release certification. The public manifest now reports `0.1.99` with macOS
> Apple silicon and Windows x86-64 artifacts.

## Current Review

No blocking issues found in the current macOS arm64 local-first build after smoke testing. The current build starts with `bluey on`, opens a compact native Bluey pill, expands into the overlay feed on click/show, starts or reuses a session, accepts typed and audio-derived transcript input, attaches readable user-selected context files, stores answer instructions, supports permissioned screenshot/page analysis, streams answer cards, detects questions/action items/decisions, returns recaps, archives sessions, and shuts down cleanly with `bluey off`. The macOS overlay is draggable, resizable, opacity-adjustable, capture-excluded, frame-persistent, click-through in the readable card area, and has a composer with model/mode selection.

The commercial path review is now explicit: `bluey on` is the intended user-facing flow, while CLI commands remain development, support, diagnostics, and smoke-test surfaces. The product now has managed auth/linking, server-side provider routing, cloud sync, and a first server-side RAG retrieval path for managed answers. Production SaaS readiness still depends on clean-machine smoke, hosted infra, payment/provider credentials, richer onboarding/settings, source cards, and cloud retention/export/delete verification.

## Known Risks

- The daemon currently uses local TCP on `127.0.0.1:57321`; this is simple for development, but a polished release should move to Unix domain sockets on macOS/Linux and named pipes on Windows.
- Local storage includes SQLite-backed sessions/search/export paths. Cloud sync and tenant-scoped managed-answer RAG now exist, but pgvector/server-owned embeddings, source-card UI, and retention/export/delete coverage still need production validation.
- The macOS overlay uses `NSWindow.sharingType = .none`, but it still needs clean-machine visual QA against Zoom/Meet/Teams and macOS screenshot/recording flows before strong public claims.
- The Windows overlay now has drag/resize hit testing, opacity command handling, bottom Recap/Search Analyse Screen/ask/mic controls, top attach/style controls, and `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` in source. The Windows audio helper cross-compiles from macOS, but overlay/audio behavior still needs real Windows hardware QA.
- The answer layer streams through live providers when configured and falls back deterministically only on explicit local/dev fallback routes. Provider-level cancellation for superseded upstream requests is still pending.
- STT has Deepgram, OpenAI Realtime, LocalWhisper, mock, and echo provider paths behind a router. Remaining risk is long-session reliability, device hot-swap, sleep/wake, permission repair, and Windows real whisper.cpp.
- Text/code/Markdown/page context attachments now store a bounded local preview. PDF and Word/RTF attachments attempt real text extraction and are rejected when they cannot be read. Analyse Screen now has a first screenshot-to-vision fallback when a vision route is configured; richer OCR/vision extraction with citations and queues is still needed.
- Screenshot capture has macOS and Windows paths. Page capture has macOS browser scripting and Windows UI Automation paths, but browser coverage needs QA.
- Periodic capture remains an explicit CLI/live support flow and records visible daemon state when used.
- Local memory/RAG exists, and synced cloud RAG now enriches managed `/router/complete` answers with account-scoped session/attachment snippets. Production RAG still needs server-owned embeddings/pgvector, citation/source cards, and stronger lifecycle tests.
- Overlay file picker and answer-style prompts are implemented for macOS first; Windows remains source/parity work until supported.
- `bluey audio ...`, `bluey ai status`, `bluey cloud ...`, `bluey providers`, `bluey memory search`, `bluey ask`, and `bluey run` are useful internal surfaces, but they are not the desired customer journey.
- Product branding has moved to Bluey, but the source-tree crate names still use `cue-*` internally to avoid a risky full-module migration in the same round.
- Production provider keys must be server-side. Any local provider key path should be treated as development-only to avoid confusing commercial setup and secret handling.
- The cloud/RAG architecture is partially implemented: authenticated sync, artifact upload, and managed-answer retrieval are wired. Remaining gaps are pgvector/vector scoring, deletion-propagation tests across all artifact/RAG tables, and a first-class cloud citation/source-card path.
- Commercial UI is incomplete without settings/onboarding, health indicators, account/workspace controls, billing, export/deletion, and a web dashboard.

## Design Checks

- The overlay is a separate process, so capture-exclusion code stays native and OS-specific.
- The CLI speaks to the daemon through a stable JSON protocol.
- Manual transcript ingestion uses the same daemon request shape that live STT will use.
- Manual context attachment uses the same meeting record shape that later screenshot/vision analysis will use.
- Permissioned screenshot capture routes into the same context attachment path.
- Page capture routes long browser pages into the same context attachment path without requiring manual scrolling.
- Overlay session control makes the context boundary explicit: continue keeps current context; new archives and starts clean.
- Periodic capture is daemon-managed and stops when requested or when the overlay exits.
- Overlay failures are tolerated by the meeting engine for transcript, recap, and manual ask paths.
- The smoke test uses isolated data/config/runtime directories and a separate daemon port so it does not mutate real local Bluey state.
- `bluey start` waits for daemon readiness, and CLI daemon errors now exit non-zero for better automation.
- `bluey on` is the normal overlay-first path; `bluey run` is a developer live-terminal path; `scripts/smoke-test.sh` is intentionally only a pass/fail health check.
- The fixed-corner diagnostic HUD has been replaced by an interactive native panel with a drag header, resize grip, darker icon controls, opacity slider, and persisted opacity.
- `bluey providers` exposes provider/key-readiness status without logging secrets.
- Overlay paperclip/notepad controls make session setup possible without remembering CLI context/instructions commands.
- Overlay ask now uses a compact macOS composer with Return-to-send, Escape-to-hide, and a Command-Shift-Space toggle.
- Overlay asks now carry requested provider, model, and answer-mode metadata into the daemon answer contract.
- Streaming answers update the active Bluey response card instead of waiting for the full provider response.
- Passive card updates no longer reopen a hidden/collapsed overlay; hidden now stays hidden until the restore pill or explicit show path is used.
- The restore pill is movable on macOS and Windows, so the minimized state is not trapped in one screen corner.
- Context attachment now rejects unsupported/unreadable files instead of saving misleading empty artifacts.
- Eye-slash hides Bluey into the small restore pill while the daemon keeps running; X/Quit asks before stopping Bluey, equivalent to `bluey off`.
- Product strategy now targets commercial managed-cloud SaaS rather than open-source/BYOK positioning.
- `docs/COMMERCIAL-PATH.md` consolidates the remaining paid-product path across user flow, UI surfaces, APIs, storage, provider keys, cloud/RAG, and gaps.

## Next Work

- Move daily-use actions behind overlay/settings UI so `bluey on` remains the only customer-facing command.
- Finish hosted auth/device registration validation, local token storage QA, and cloud sync smoke against staging/prod.
- Clean-machine validate the macOS arm64 tarball, installer script, `bluey on`, and `bluey off`.
- Harden audio/STT with long-session stress, sleep/wake, device hot-swap, permission repair, and Windows hardware QA.
- Add provider cancellation, citations, and stronger answer metadata behind the existing answer request/response/event contract.
- Add server-owned embeddings/pgvector, richer OCR/vision extraction, source cards, and tenant-scoped RAG lifecycle tests.
- Create settings/onboarding and web dashboard surfaces for account, permissions, workspace, billing, history, export, and deletion.
- Add crash diagnostics, opt-in telemetry/support bundle export, and eventually signed desktop apps with auto-update.
