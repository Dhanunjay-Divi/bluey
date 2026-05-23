# Bluey Worklog

This file is the running implementation history. When new work extends an existing theme, append it to that round. When the product gains a new capability area, create the next round.

## Round 1: Native Foundation

Built the first Rust-native project structure:

- Added the Rust workspace with `cue-core`, `cue-cli`, and `cue-daemon`.
- Added local daemon IPC over JSON/TCP for simple development.
- Added durable daemon state and per-user app paths.
- Added macOS and Windows native overlay sidecar sources.
- Added documentation for architecture, roadmap, references, first-version testing, and self-review.

## Round 2: Usable Meeting Loop

Turned the plumbing into a usable manual first version:

- Added meeting lifecycle commands.
- Added manual transcript ingestion through `bluey listen`.
- Added deterministic local detection for questions, action items, and decisions.
- Added `bluey ask`, `bluey recap`, and `bluey action-items`.
- Added `bluey run` and `scripts/run-local.sh` for an interactive live terminal session.
- Changed `scripts/smoke-test.sh` from noisy demo output into pass/fail assertions.

## Round 3: Interactive Private Overlay

Made the overlay behave like a product surface instead of a fixed diagnostic HUD:

- Made the macOS overlay movable, resizable, and frame-persistent.
- Added a drag header and resize grip.
- Added darker header icon controls.
- Added overlay clear/hide controls.
- Added opacity slider, dim/brighten buttons, and persisted opacity.
- Added live commands for `/opacity`, `/position`, and `/privacy`.
- Re-applied capture exclusion on show/render.
- Updated Windows overlay source with drag/resize hit testing and opacity command handling.

## Round 4: Ethical Context And Provider Readiness

Mapped the reference-app feature checklist into ethical meeting-copilot scope:

- Added `docs/FEATURE-MAP.md` with explicit non-goals.
- Added user-selected context attachments for screenshots, diagrams, code files, documents, text, and other files.
- Added `bluey context add`, `bluey context list`, live `/attach`, and live `/context`.
- Added context-aware answers and recaps.
- Added `bluey providers` for provider/key readiness without exposing secrets.
- Expanded smoke tests to verify context attachment.

## Round 5: Permissioned Screen Context

Started the safe screen-context pipeline:

- Added an explicit, user-triggered screenshot capture flow.
- Captures are saved into Bluey's per-user data directory and attached to the active meeting as context.
- The default macOS flow uses interactive `screencapture -i`, so the user chooses the window/region.
- The capture flow can open a local preview and asks for confirmation before attaching unless skipped with `--yes`.
- Added a visible eye control in the macOS overlay.
- Clicking the eye shows a consent popup before starting periodic screen context capture.
- Clicking the eye again stops periodic capture.
- Added daemon-managed screen capture state so capture continues only while explicitly active.
- Added `bluey context watch start --interval <seconds>`, `bluey context watch stop`, live `/capture-on`, and live `/capture-off`.

## Round 6: Easier Session Setup

Reduced the need to remember terminal commands:

- Added a paperclip button to the macOS overlay for selecting session files with the native file picker.
- Added an answer-style button to the overlay for setting how Bluey should answer in this session.
- Added `bluey instructions set/show/clear` for terminal fallback.
- Added live `/style ...` for terminal live mode.
- Stored answer instructions in the meeting record and included them in recaps.
- Expanded smoke tests to verify answer instructions.
- Added `docs/SESSION-FLOW.md` to describe the overlay-first workflow and keep CLI commands as fallback/testing tools.

## Round 7: Commercial Cloud And RAG Direction

Shifted the product direction toward a paid SaaS:

- Added `docs/PRODUCT-STRATEGY.md` for commercial positioning, secure cloud storage, managed providers, billing, workspace accounts, and enterprise controls.
- Added `docs/CLOUD-RAG.md` for production RAG memory design.
- Added `bluey memory search` as the app-facing RAG/memory surface.
- Added daemon memory search across active/archived meetings, transcripts, context, decisions, action items, and answer instructions.
- Expanded smoke tests to verify memory search.
- Kept local search as a development slice; production storage should move behind authenticated cloud sync and tenant-scoped retrieval.

## Round 8: Overlay Polish And Competitive Gaps

Refined the overlay controls and tracked remaining product gaps:

- Removed the separate opacity minus/plus buttons; the opacity slider is the single control.
- Changed the answer-style control to a notepad-style button.
- Added a close button that asks for confirmation before stopping Bluey.
- Added `docs/COMPETITIVE-GAPS.md` to track missing product features against the broader market.

## Round 9: Product Pipeline Scaffolding

Added the code seams needed to turn the prototype into the paid product architecture:

- Added `audio.rs` with dual system/microphone capture planning, source/device/format state, chunk metadata, STT segment metadata, and audio events.
- Added daemon-visible audio runtime status plus `bluey audio status`, `bluey audio start`, and `bluey audio stop`.
- Added `ai.rs` provider routing primitives for managed Bluey routing, fallback providers, latency/cost budgets, privacy/safety flags, answer requests/responses, streaming events, and provider health.
- Added `bluey ai status` so routing readiness is visible from CLI/live mode before live provider calls are wired.
- Added `cloud.rs` with cloud identity, device identity, auth/sync state, encrypted artifact upload metadata, retention policy, sync events, RAG query/result/citation models, memory chunks, export, and deletion request types.
- Added `bluey cloud status` and `bluey cloud sync` as the secure cloud/RAG control surface.
- Expanded the smoke test to assert audio, AI routing, and cloud scaffolds.
- Added `docs/IMPLEMENTATION-SEAMS.md` to show where native capture, STT, managed providers, cloud sync, and RAG plug in next.

## Round 10: One-Command Overlay Entry

Moved the user-facing flow closer to the intended product:

- Added `bluey on --title ...` as the normal entrypoint.
- `bluey on` starts the daemon quietly, starts or reuses a meeting, shows the overlay, and renders a short boot animation.
- Added an overlay `boot` command to macOS, with a lightweight terminal-style line reveal.
- Added Windows overlay handling for the same `boot` command as a static boot panel.
- Kept audio, AI, and cloud commands as developer/debug surfaces rather than the main user flow.
- Updated smoke tests and docs so the normal path starts with `bluey on`.

## Round 11: Overlay Ask Flow

Removed another terminal dependency from the product path:

- Added an overlay ask event to the shared native protocol.
- Added a speech-bubble ask button to the macOS overlay.
- The ask button opens a native question popup and emits the question to the daemon.
- The daemon now routes overlay questions through the same meeting-aware answer path as `bluey ask`.
- Answers render back into the overlay as answer cards with `overlay ask` source metadata.
- Updated docs/gap tracking to mark overlay ask popup as implemented and inline composer as the next UX upgrade.

## Round 12: Modal Layering And Deployment Plan

Improved product usability and operating plan:

- Fixed macOS icon popups by presenting ask, attach, answer-style, capture, and close dialogs above the high-level overlay.
- Moved paperclip file selection into the overlay process so the picker is not hidden behind Bluey.
- Moved answer-style input into the overlay process so the notepad dialog stays foreground.
- Added direct overlay events for selected file paths and answer-style updates.
- Improved overlay card readability with per-kind accents for answers, context, actions, decisions, and warnings.
- Added `docs/DEPLOYMENT-SCALING.md` covering desktop packaging, cloud APIs, storage, provider keys, workers, scaling, and product priorities.

## Round 13: Commercial Path Review

Consolidated the remaining paid-product path:

- Added `docs/COMMERCIAL-PATH.md` to make the product boundary explicit: `bluey on` is the only intended user-facing flow, while CLI commands are internal development, diagnostics, support, smoke-test, and automation surfaces.
- Documented the required commercial UI surfaces: native overlay, settings/onboarding, web dashboard, and support/diagnostic CLI.
- Documented the cloud APIs needed for auth, device registration, meetings/events, answer streaming, artifact upload, sync, RAG, settings, billing, export, deletion, and audit access.
- Documented the storage split between local SQLite/cache/credential storage and cloud Postgres/object/vector/queue/KMS infrastructure.
- Clarified that production provider keys belong server-side behind managed Bluey cloud routing; local provider keys are development-only.
- Expanded the cloud/RAG path with active-session retrieval, workspace memory retrieval, citations, and retention/deletion requirements.
- Updated `docs/SESSION-FLOW.md`, `docs/CLOUD-RAG.md`, `docs/DEPLOYMENT-SCALING.md`, and `docs/SELF-REVIEW.md` to keep the commercial path, risks, and next work aligned.

## Round 14: Composer, Answer Runtime, And Context Preview

Turned more of the daily workflow into actual product UI and tightened the answer pipeline:

- Replaced the macOS ask popup with an inline overlay composer.
- Added Return-to-send, Escape-to-hide, and Command-Shift-Space composer/show behavior on macOS.
- Kept the composer inside the overlay layout so cards and input do not overlap.
- Added richer answer IPC with `AnswerRequest`, `AnswerResponse`, stream events, provider/model route metadata, usage estimates, cost estimates, and safety notices.
- Kept the live answer runtime honest: it records requested provider metadata but still uses the local deterministic answer until managed provider adapters are implemented.
- Added local text/code preview extraction for attached context files so first-version answers can use more than file names and notes.
- Marked image, screenshot, PDF, and office-document extraction as pending for OCR/vision/document workers.
- Updated commercial docs, feature gaps, and self-review to reflect the new composer, answer contract, and remaining production gaps.

## Round 15: Command Bar And Model Selection UI

Moved the macOS overlay closer to the product UI target:

- Expanded the overlay into a wider translucent command bar with a persistent ask field.
- Added model route selection for Bluey Auto, OpenAI reasoning, Groq realtime, Cerebras fast, and local fallback.
- Added answer modes for General, Code, System Design, Meeting, and Writing.
- Added quick action chips for Answer, Shorten, Recap, Follow-up, and Context.
- Kept capture, attach, answer rules, opacity, clear, hide, and close controls in the same command surface.
- Updated overlay ask events to carry requested provider/model/mode metadata to the daemon.
- Updated the daemon to build model-aware `AnswerRequest` values from overlay asks while still using the honest local runtime until live adapters are wired.
- Updated product docs to mark macOS model/mode selection as implemented and provider adapters as the next production step.

## Round 16: Production Hardening Backbone

Built the first real production-facing backbone across runtime, providers, and deployment contracts:

- Added provider adapter contracts for request payloads, provider configuration, model fallback, endpoint/key readiness, and route metadata.
- Added OpenAI-compatible HTTP chat completion calls for OpenAI, Groq, Cerebras, and optional Bluey-managed-compatible endpoints when credentials are present.
- Kept default Bluey Auto reliable offline by adding an explicit local fallback after managed/provider routes.
- Added richer AI status and CLI metadata so missing keys/endpoints are visible without logging secrets.
- Added an audio runtime session with simulated development PCM chunks when native capture is unavailable.
- Simulated audio now emits labeled `[dev audio:system]` and `[dev audio:microphone]` transcript segments through the same meeting pipeline that streaming STT will use.
- Added platform audio capability reporting, runtime mode, session id, source chunk counters, and emitted transcript counts.
- Added backend/deployment skeleton files under `infra/`: OpenAPI contract, Postgres/pgvector migration outline, worker queue definitions, and environment contract.
- Added settings UI, backend contract, and installer checklist docs for production rollout.
- Expanded smoke testing to wait for the simulated audio runtime and verify audio counters.

## Round 17: Pre-Pricing Product Review

Consolidated the state before pricing discussions:

- Added `docs/PRE-PRICING-REVIEW.md` with current working model, missing reference-app capabilities, product differentiators, explicit non-goals, and plan-enforcement areas.
- Updated `docs/REFERENCE-MAP.md` with Bluey's current status against the reference packages.
- Updated `docs/COMPETITIVE-GAPS.md` to reflect the simulated audio runtime, provider HTTP path, and backend/deployment skeleton.
- Linked the pre-pricing review from `README.md`.

## Round 18: Customer CLI And Overlay UX Cleanup

Reduced the daily surface to the intended product flow:

- Added `bluey off` as the customer-facing shutdown command.
- Hid internal/dev commands from normal `bluey --help`; the visible commands are now `on`, `off`, and `help`.
- Kept internal commands available for smoke tests, diagnostics, and support.
- Moved the ask input from the top command bar into a bottom tray.
- Added a recording start/stop mic button to the overlay header.
- Added a green recording dot that brightens when recording is active.
- Wired overlay recording start/stop events into the daemon audio runtime.
- Updated README and session-flow docs so users are guided to `bluey on`, overlay controls, and `bluey off`.

## Round 19: Bottom Tray Command Center

Made the most important controls visible where the user is already typing:

- Made the macOS bottom ask tray visible by default instead of starting collapsed.
- Moved recording start/stop out of the header and into the bottom tray.
- Added a larger bottom mic button with a green active-state dot.
- Added a larger bottom send button next to the ask field.
- Kept the ask field open after sending so repeated questions do not require reopening the composer.
- Simplified the header so it focuses on model/mode selection, context setup, opacity, hide, and close.
- Kept Escape from collapsing the tray; it now only releases text focus.
- Updated README and session-flow docs to describe the bottom-tray-first customer flow.

## Round 20: Terminal Product And Windows Parity Path

Aligned Bluey with the terminal-first product direction borrowed from Pinky while keeping Bluey's overlay live surface:

- Added terminal account/session/settings commands in this round. These were
  later hidden from the customer help surface so `bluey on` / `bluey off`
  remain the only normal commands.
- Kept `bluey on` and `bluey off` as the normal live-session loop.
- Added local account/settings JSON under Bluey's config directory with private file permissions on Unix.
- Added browser-callback login plumbing compatible with Bluey/Pinky-style `/login?callback=...&state=...` flows.
- Added hidden dev/non-browser account linking for token and local-only tests.
- Made daemon cloud status read saved Bluey account config in addition to environment variables.
- Added local session history listing/detail output through `bluey sessions`.
- Added terminal-first settings for answer style, model label, mode label, opacity, cloud sync preference, and retention.
- Applied saved answer style and overlay opacity when `bluey on` starts a session.
- Upgraded the Windows overlay from a passive card window to an interactive Win32 overlay with ask/send, mic, eye, attach, style, close, drag/resize, opacity, and capture-exclusion support.
- Added Windows file picker, answer-style prompt, and full-screen screenshot capture via normal user-level PowerShell/Win32 APIs.
- Added `scripts/build-macos.sh` and `scripts/build-windows.ps1` for unsigned terminal-first dev/release folders.
- Added the Rust Windows GNU target locally and verified `cargo check --target x86_64-pc-windows-gnu`.
- Cross-compiled the Windows overlay C source with MinGW on macOS as a syntax/link sanity check.

## Round 21: Page Context Without Scrolling

Implemented the consent-based version of the "read the whole question without scrolling" flow:

- Added a Page quick chip to the macOS bottom tray and a Page button to the Windows overlay.
- Added a confirmation dialog before Bluey asks a browser for page text.
- Added shared overlay and daemon IPC events for active page capture.
- Added a hidden support fallback command: `bluey context page`.
- Implemented macOS browser page extraction for Chrome-family browsers and Safari through normal browser scripting APIs.
- Implemented a Windows user-level UI Automation backend for supported browser document text when exposed by the browser.
- Preferred the frontmost supported browser when macOS exposes it, then fell back through other running supported browsers.
- Extracted likely problem/content DOM regions first, then fell back to main/body text when the page structure is unfamiliar.
- Saved captured page text into Bluey's data directory under `page-context/` and attached it as a ready text context artifact for the active session.
- Pushed an overlay card showing the attached page title and captured character count.
- Wired the Windows overlay/protocol path; Windows page-text extraction still needs real Windows hardware QA, while screenshot capture and file attach remain available as fallback context flows.

## Round 22: Session Control And Mental Model

Made the active session model visible in the overlay:

- Added a small Session control to the macOS overlay header.
- Added a Session button to the Windows overlay header.
- Added overlay events for continuing the active session or starting a clean new session.
- Continue keeps the current transcript, answer rules, attached files, page captures, and screenshots.
- New Session archives the current session first, then starts a clean session for fresh documents, page context, audio, and answers.
- Added overlay cards that confirm whether Bluey continued an existing session or started a new one.

## Round 23: Click-Through Reading Area

Made the overlay less blocking while keeping controls usable:

- Added macOS root hit testing so only the header, resize edges, and bottom composer accept pointer events.
- The macOS middle answer/card area now passes clicks through to the app underneath Bluey.
- Updated Windows `WM_NCHITTEST` so the header and bottom composer remain interactive while the middle returns transparent hit testing.
- Kept resize edges interactive on both platforms.

## Round 24: Control Legend And Transcript Cards

Made the overlay easier to understand and made audio transcript output visible:

- Added a question-mark help control to the macOS header.
- Added a Help button to the Windows header.
- Added in-app legend text for the green status dot, recording dot, all header controls, bottom tray controls, quick actions, and card types.
- Added `docs/ICON-GUIDE.md` as the durable reference for every icon and dot.
- Added a `transcript` card kind for source-labeled audio/STT output.
- Audio transcript segments now render as overlay cards labeled as system audio or microphone transcript.
- Kept the audio path honest: current local runtime can simulate source-labeled STT when native capture/STT is unavailable, and real capture can plug into the same transcript-card path.

## Round 25: Background After Overlay Close

Matched the Pinky-style background behavior:

- Changed the macOS X control to hide the overlay instead of stopping Bluey.
- Added a macOS `windowShouldClose` guard so window close hides the overlay and keeps the overlay process alive.
- Changed the Windows Close button and `WM_CLOSE` path to hide the overlay instead of destroying the window.
- Changed daemon handling for `close_requested` to hide the overlay instead of shutting down the daemon.
- Kept `bluey off` as the explicit customer-facing command for stopping Bluey.

## Round 26: Bluey Product Branding

Switched the customer-facing product name from Cue to Bluey while preserving the old command names as compatibility aliases:

- Added `bluey` and `bluey-daemon` binaries alongside the existing `cue` and `cue-daemon` development aliases.
- Renamed overlay-facing labels, boot cards, dialogs, help text, model labels, provider IDs, and default managed/local model names to Bluey.
- Added `BLUEY_*` data/config/runtime/daemon/overlay/cloud environment variables with `CUE_*` fallback compatibility.
- Changed default local app directories to `bluey`, while reusing an existing legacy `cue` directory if Bluey state has not been created yet.
- Updated macOS and Windows overlay build outputs to produce `bluey-overlay-*` artifacts while still copying old `cue-overlay-*` aliases.
- Updated smoke tests, release folder names, README, session flow, icon guide, and first-version test docs for the `bluey on` / `bluey off` flow.
- Kept crate/module names like `cue-core` in place for now so the rename stays product-facing and low-risk instead of becoming a disruptive source-tree migration.

## Round 27: Collapsed Overlay Reopen Path

Made hide/close behavior understandable without needing a terminal command:

- Changed macOS eye-slash, window close, daemon hide, and toggle-hide paths to collapse the overlay into a small floating Bluey button instead of disappearing completely.
- Clicking the small Bluey button restores the full overlay and focuses the bottom ask tray.
- Changed the macOS X confirmation text to explain that Bluey keeps running, sessions are saved for dashboard/history, and `bluey off` is the full stop command.
- Added the same collapsed-button behavior to the Windows overlay by shrinking the topmost window into a small Bluey pill and restoring it on click.
- Updated README, session flow, first-version test, and icon guide docs so users know how to unhide Bluey.

## Round 28: Popup Placement Away From Overlay

Kept OS dialogs from covering the Bluey frame:

- Added macOS modal placement logic that positions alerts and file pickers beside or away from the overlay instead of centered on top of it.
- Applied that placement to help, session, page capture, file attach, answer-style, close, and screen-capture confirmation dialogs.
- Added Windows message-box placement logic using a thread-local activation hook so help/session/capture/page/close popups open away from the overlay window.
- Collapsed the overlay before daemon-owned file-picker and answer-style prompts, then restored it on cancel so Windows/PowerShell dialogs are not hidden by the topmost overlay.

## Round 29: Collapsed Pill Expand Reliability

Fixed the collapsed overlay restore path:

- Replaced the macOS collapsed pill's embedded button with a custom clickable view so the whole Bluey pill restores the overlay on click release.
- Made the collapsed macOS pill key-capable and first-responder friendly, so Enter/Space can also restore it when focused.
- Redrew the macOS collapsed pill directly in the view instead of relying on nested Auto Layout controls inside a borderless non-activating panel.
- Updated the Windows collapsed pill to restore on mouse-down as well as mouse-up, making the tiny restore target more forgiving.

## Round 30: Overlay Action Simplification

Simplified the action surface around what should actually ship in the first usable build:

- Removed the visible Shorten, Follow-up, and bottom Context quick actions from the macOS tray.
- Changed Recap from a canned AI prompt to a first-class overlay event that calls the daemon recap path and renders the session recap card.
- Renamed the active browser page action to Capture, with copy that explains it captures readable page text for answers without manual scrolling.
- Removed the visible top eye capture control from the macOS header and Windows header so users do not confuse passive screen capture with active page capture.
- Changed the attach control into an attachments menu: attach new files or show the documents, screenshots, page captures, and notes already attached to the session.
- Added matching Windows overlay events for Recap, Capture, and Show Attached.

## Round 31: Hide Wording And Dead Capture Control Cleanup

Aligned the close/hide language with the real behavior:

- Changed the macOS X tooltip and confirmation dialog from Close overlay to Hide overlay.
- Changed the Windows header button label from Close to Hide and updated its confirmation dialog.
- Updated docs to describe X/Hide as collapsing Bluey into the small restore button.
- Removed stale native overlay code for the old visible eye capture button on macOS and Windows; support-level capture commands remain in the daemon for later/testing.

## Round 32: Quit Semantics And Analyse Bar

Separated hide from quit and tightened the bottom composer:

- Changed eye-slash to remain the hide/collapse path and changed X/Quit into the true `bluey off` path from the overlay.
- Added an overlay `analyze_screen_requested` event that hides the overlay, captures readable active-browser page text, attaches it as context, and then generates an answer.
- Changed the bottom Capture quick action to Analyse so it produces an answer instead of only attaching context.
- Made the bottom composer shorter, centered, and hanging instead of spanning the full chat width.
- Gave the bottom action buttons equal widths and kept the mic/send controls as icon-first buttons.
- Prevented long card titles, sources, or URLs from horizontally stretching the macOS overlay by lowering label compression resistance and constraining card label widths.
- Improved macOS browser selection so a detected frontmost browser is used exclusively instead of silently falling through to Safari if that frontmost browser fails.

## Round 33: Real Chunked Audio And STT Runtime

Moved audio from scaffold-only into a working end-to-end path:

- Added a native audio pipeline status constructor so Bluey can report real capture separately from simulated development audio.
- Added an FFmpeg-backed runtime that captures short source-specific WAV chunks, sends them to an OpenAI-compatible transcription endpoint, deletes the transient audio file, and emits `SttSegmentMetadata` into the existing transcript/session/overlay path.
- Added first FFmpeg AVFoundation device discovery as a temporary fallback path for macOS system and microphone audio.
- Added Windows command paths for DirectShow microphone capture and WASAPI loopback system capture so the code compiles for Windows and is ready for hardware QA.
- Added STT configuration through `OPENAI_API_KEY` or `BLUEY_STT_API_KEY`, plus optional `BLUEY_STT_API_URL`, `BLUEY_STT_MODEL`, `BLUEY_STT_CHUNK_MS`, `BLUEY_FFMPEG_PATH`, `BLUEY_MIC_AUDIO_DEVICE`, and `BLUEY_SYSTEM_AUDIO_DEVICE`.
- Kept the development simulator as an explicit fallback when FFmpeg, devices, or STT credentials are unavailable so smoke tests and offline demos still exercise the same transcript flow.
- Changed the overlay send/Answer behavior so an empty ask field answers the latest clear question from transcript, active page context, and attached files instead of just beeping.
- Updated README, first-version test, feature map, session flow, implementation seams, pre-pricing review, and self-review docs to reflect the real audio/STT path and remaining production hardening.

## Round 34: Compact Dark Command Bars

Tightened the top and bottom overlay controls toward the compact Bluey command-bar direction:

- Removed the bright/default control feel from the bottom composer by using a darker translucent capsule, subtle border, and smaller centered width.
- Reduced the composer height from 128px to 112px and changed the ask placeholder to "Ask me anything..." for the reference-style input.
- Made macOS top icon controls larger, circular, evenly spaced, and consistently dark.
- Made the bottom mic/send buttons smaller round controls so the input row reads as one floating bar.
- Centered and equalized the Answer, Recap, and Analyse quick-action chips.
- Restyled macOS model/mode pickers to match the darker rounded command-bar surface.
- Updated the Windows overlay source to use dark owner-drawn buttons and a dark edit field instead of default white Win32 controls, with centered button labels and a centered compact composer width.

## Round 35: Native Audio Helpers

Removed loopback-driver setup from the primary audio path:

- Added `native/macos/cue-audio`, a Swift helper that captures system audio through ScreenCaptureKit and microphone audio through CoreAudio/AVFoundation, emitting short raw PCM chunks for the Rust daemon.
- Added `native/windows/cue-audio`, a WASAPI helper that captures default system loopback audio and default microphone audio as raw PCM chunks for the Rust daemon.
- Changed the daemon to prefer bundled native helpers whenever STT credentials are configured, while keeping FFmpeg as a fallback/dev path.
- Converted helper PCM output into transient 16 kHz mono WAV chunks before sending the same OpenAI-compatible STT request path, preserving the existing source-labeled transcript/session/overlay flow.
- Added helper discovery through `BLUEY_AUDIO_HELPER_BIN`/`CUE_AUDIO_HELPER_BIN`, packaged-helper lookup, and local debug build lookup.
- Updated macOS and Windows build scripts, run-local, and smoke-test scripts to build and package audio helpers alongside overlay helpers.
- Updated README and product docs so the scalable story is native audio first, not third-party loopback/FFmpeg first.

## Round 36: Product Visual Polish And Bluey Brand

Moved Bluey closer to a product-grade visual identity:

- Added a dedicated Bluey logo mark in `assets/bluey-logo.svg` and reused it for the static web starter under `web/assets/bluey-logo.svg`.
- Restyled the macOS overlay with a branded header mark, stronger blue/cyan glass palette, clearer card styling, a visible resize frame, and a larger interactive bottom-right resize grip.
- Made the collapsed macOS pill show the Bluey mark and wordmark instead of a plain `B`.
- Restyled the Windows overlay paint path with the Bluey mark, darker glass-like header/composer surfaces, a visible resize border, and a larger collapsed pill.
- Added `web/index.html`, a static bluey.sh landing-page starter with an animated product scene, product mockup, platform/security sections, and the `stay present, stay unseen` caption.

## Round 37: Bluey Logo And Action Bar Polish

Refined the overlay toward the latest product mockup direction:

- Replaced the old eye-like mark with a dedicated Bluey `b` signal mark in native macOS, native Windows, and the web starter assets.
- Removed Recap from the top command header so the header stays focused on session, model/mode, attachment, style, hide, and quit controls.
- Kept Recap in the bottom composer where it belongs beside the primary Answer and Analyse Screen actions.
- Renamed Analyse to Analyse Screen/Search wording where needed, with confirmation copy that explains Bluey reads the active browser page or available screen context before generating an answer.
- Reworked the macOS bottom chips into custom centered controls so icons and labels align cleanly instead of inheriting uneven default button styling.
- Added spacing between the macOS card area and bottom composer so the composer reads as a separate hanging action bar.
- Adjusted the Windows bottom composer so Recap and Search / Analyse Screen live below the ask row, outside the top header.
- Updated the bluey.sh static mockup so the bottom action bar is visually separated from the main response/transcript panel.

## Round 38: Command Spark Logo And Mockup Cleanup

Took the selected option 3 direction further after visual review:

- Promoted the Command Spark mark into the shared Bluey SVG assets used by the app/website.
- Updated the native macOS and Windows drawn logo marks so the overlay no longer shows the old cramped `b` badge.
- Rebuilt the web mockup top command bar with real icon buttons instead of empty circular placeholders.
- Changed the bottom mockup action from the overlong `Search / Analyse Screen` label to a cleaner icon-backed `Analyse Screen` chip.
- Resized the top and bottom mockup bars so text stays centered and does not wrap awkwardly.

## Round 39: Logo And Landing Page Update

Applied the selected identity to the shipping-facing web assets:

- Kept the Command Spark mark as the canonical static logo in `assets/bluey-logo.svg` and `web/assets/bluey-logo.svg`.
- Added the `bluey on` animated Command Spark GIF to `web/assets/bluey-on.gif` for web use.
- Updated the landing-page hero so `Bluey` is the first-viewport product signal and `stay present, stay unseen` is the caption.
- Added a compact `bluey on` launch badge to the hero to connect the website directly to the terminal-first product flow.

## Round 40: Overlay Pipe Recovery

Fixed a real startup failure found while running `./target/debug/bluey on`:

- The daemon could keep a stale overlay stdin handle after the native overlay had already exited.
- `bluey on` then returned a raw `Broken pipe (os error 32)` when sending the boot card.
- Added overlay liveness checks before every overlay command.
- Added one-shot overlay restart and command retry when a pipe write fails.
- Made overlay exit events ignore stale old processes when a replacement overlay is already running.
- Verified normal detached `bluey on`, foreground daemon mode, and smoke tests after the fix.

## Round 41: Right-Weighted Black Glass Polish

Matched the landing-page mockup and native overlay more closely to the latest reference:

- Shifted the bluey.sh hero mockup into a tighter right-side product cluster so the top command bar, response panel, and bottom composer read as one floating system.
- Darkened the page toward a pure-black product background with subtler blue/cyan wave detail.
- Rounded the web top command bar, main panel, and bottom composer into larger capsule/card shapes with closer vertical spacing.
- Replaced the visible tiny GIF badge with a crisp SVG logo plus CSS pulse because the exported GIF became jagged and hard to read at 32px.
- Kept `web/assets/bluey-on.gif` as an export asset, but stopped relying on it for the primary hero badge.
- Updated the macOS overlay shell, header capsule, composer capsule, text field, and resize frame to use the same darker rounded visual language.
- Updated the Windows overlay paint/layout path with a centered rounded top capsule, darker composer, black edit field, and matching control geometry.
- Verified the static site through a local browser preview and saved the screenshot at `tmp-bluey-web-tight.png`.

## Round 42: Reliable Detached On/Off

Hardened the product command path after local testing showed stale runtime state could be confusing:

- Changed detached daemon launch to create a separate process group on Unix/macOS and a detached process group on Windows.
- This makes `bluey on` survive the parent terminal/tool process more reliably instead of only working while the launcher process stays alive.
- Changed `bluey off` so if the daemon is already gone but a stale `daemon-state.json` remains, Bluey removes that stale state and reports `Bluey is off.` instead of surfacing a scary connection-refused shutdown error.
- Verified `./target/debug/bluey on` leaves a listening daemon on `127.0.0.1:57321` and `./target/debug/bluey ask "ping"` reaches it after the launcher exits.

## Round 43: Conversation Feed, Theme Toggle, And Resize Polish

Turned the overlay interaction area into a clearer working tool:

- Added a `question` card kind so Bluey now shows a sent-question card before the answer arrives.
- Changed answer cards from `Q: ...` titles to clean `Answer` cards so questions and answers read as separate chronological items.
- Changed the macOS overlay feed from newest-at-top insertion to chronological scrolling with auto-scroll to the newest card.
- Increased retained visible card history to 60 cards so longer sessions remain usable without losing recent context too quickly.
- Added a black/white background toggle to the macOS header and Windows header while preserving Bluey cyan borders.
- Added the same black/white background switch to the web mockup for visual review.
- Improved macOS resize behavior by enlarging interactive resize edges and clamping the bottom-right grip against the visible screen instead of letting the frame hit awkward partial limits.
- Replaced Unix process-group detachment with `setsid()` for the CLI daemon launch so `bluey on` survives parent process cleanup more reliably.
- Verified `bluey on`, daemon reachability, question/answer card push path, macOS overlay build, Windows overlay syntax, Rust tests, and the web theme toggle preview.

## Round 44: Hide Stability, Readable Context Policy, And Handoff Notes

Tightened the product behavior after overlay and context review:

- Fixed the hide bounce path: passive `push_card` updates now render into the feed without reopening a hidden/collapsed overlay.
- Added a short click debounce to the collapsed macOS restore pill so the same click that hides Bluey cannot immediately reopen it.
- Kept explicit `show`, `boot`, and user open actions as the only daemon paths that mark the overlay visible.
- Changed the Windows overlay stdin path so passive cards do not reopen a collapsed/hidden window either.
- Brightened the macOS bottom Answer, Recap, and Analyse Screen chips with stronger Bluey blue/cyan borders and icon tint so they read as primary actions.
- Added a vertically centered single-line text-field cell for the macOS ask composer so the placeholder and entered questions sit cleanly in the input capsule.
- Added a stricter session-context policy: readable text, Markdown, and code are parsed locally; PDF and Word/RTF documents attempt real text extraction; unsupported or unreadable files are rejected instead of silently becoming fake context.
- Added macOS file-picker restrictions and Windows picker filters for readable Bluey context files.
- Changed answer context assembly so only ready/readable artifacts, or explicit user notes attached to a pending item, are passed into answer generation.
- Added context status/error output in the CLI and attached-context overlay card so future agents can see whether a file is ready, pending, failed, or unsupported.
- Added `docs/HANDOFF.md` as the durable next-agent/product handoff for current state, required keys/models, local testing commands, and remaining production work.

## Round 45: Movable Hidden Pill And Theme Toggle Polish

Made the hidden/minimized experience feel like a real product control instead of a fixed afterthought:

- Made the macOS collapsed Bluey pill draggable; a click restores Bluey, while dragging moves the pill and saves the minimized position.
- Clamped the saved macOS minimized pill position to the visible screen so monitor changes do not strand it off-screen.
- Made the Windows collapsed Bluey pill draggable with click-to-restore and drag-to-move behavior.
- Added a theme toggle directly into the web mock overlay top bar so the black/white UI switch is visible where the real app controls live.
- Synced the web nav theme switch and mock overlay theme switch so either one toggles the full page and product mockup together.
- Added blue/cyan focus styling for the web theme switches and prevented the `bluey on` launch chip from wrapping.
- Rebuilt the macOS overlay and verified the web light/dark toggle behavior in the in-app browser preview.

## Round 46: Overlay-Only Theme Scope

Corrected the theme behavior after product review:

- Scoped the web black/white toggle to the Bluey overlay mockup only.
- Kept the website/page background, hero copy, navigation, and lower sections unchanged when the toggle is clicked.
- Renamed the website toggle copy from page background language to UI language so it is clear the switch controls the overlay surface.
- Kept the nav toggle and overlay top-bar toggle synced while applying the light class only to the product mockup.
- Improved light-overlay icon contrast so top-bar controls remain readable on the white overlay surface.
- Updated the native minimized pill paint path so the hidden/restored Bluey pill follows the selected black/white overlay theme on macOS and Windows.

## Round 47: Opposite Backdrop Showcase Pairing

Adjusted the web showcase theme pairing to make both UI modes legible:

- White Bluey UI now displays on the black Bluey site background.
- Dark/black Bluey UI now displays on a white/light showcase background.
- The toggle still controls the Bluey UI mode, but the surrounding hero backdrop flips only to provide contrast for that mode.
- Restored high-contrast white text inside the dark overlay cards when the surrounding page is light.
- Verified both browser states: dark UI on white backdrop and white UI on black backdrop.

## Round 48: Analyse Screen No-Hide Path

Aligned the Analyse Screen UX with the capture-excluded overlay behavior:

- Removed the daemon-side automatic overlay hide before active browser page analysis.
- Removed the macOS pre-hide call from the Analyse Screen confirmation flow.
- Updated macOS and Windows confirmation copy to explain that Bluey reads available page/screen context and that the overlay is excluded from normal screen capture.
- Documented that the current Analyse Screen path is browser-readable text capture, while any future screenshot/OCR fallback should decide separately whether a temporary hide is needed.

## Round 49: Analyse Screen Vision Fallback

Made Analyse Screen recover when browser text cannot be read:

- Reviewed the local reference packages for their capture pattern: OpenCluely captures a screen thumbnail and sends the image buffer to a vision model; Aura keeps a screenshot queue and sends images as OpenAI-compatible `image_url` parts with provider/key failover; Pinky documents native capture preflight and platform stability lessons.
- Changed Analyse Screen to try active browser page text first, then fall back to one user-confirmed screenshot when page text is unavailable.
- Added OpenAI-compatible multimodal chat payload support in the daemon so screenshot context can be sent as `image_url` content parts.
- Added a vision route selector: explicit `BLUEY_VISION_PROVIDER`/`BLUEY_VISION_MODEL`, then OpenAI when `OPENAI_API_KEY` is set, then Bluey managed vision when cloud compatibility is enabled. Groq can be used for vision when an explicit vision model is configured.
- Added a clear `Analyse needs vision` warning card when page text fails and no vision provider is configured, instead of pretending screenshots are answer-grade context.
- Updated the docs to describe the page-text-first, screenshot-vision-second flow.

## Round 50: Source-Labeled Conversation Feed

Clarified how live audio, typed questions, answers, and follow-ups appear in the overlay:

- Reviewed the reference UI patterns again: Aura streams interviewer/candidate/AI messages into one conversation feed, while OpenCluely separates chat and response windows.
- Kept Bluey as one scrollable feed so the user sees `SYSTEM`, `MIC`, `YOU`, and `BLUEY` events in chronological order.
- Shortened daemon transcript card titles from long phrases to `System` and `Mic`.
- Changed typed questions to create a `YOU` card before routing the answer.
- Changed answer cards to render as `BLUEY` cards immediately after the matching question.
- Polished the macOS overlay renderer so transcript rows are compact source-labeled chips, while question and answer cards stay larger and easier to scan.
- Updated the Windows overlay latest-card renderer to use the same `SYSTEM`/`MIC`/`YOU`/`BLUEY` labels.
- Added durable recent Q&A memory to the session record so a later follow-up answer receives the previous `YOU`/`BLUEY` turns, not only raw transcript and attached files.
- Added recent Q&A to recap/local fallback output so development mode reflects the same follow-up continuity.
- Kept Analyse Screen model prompts detailed internally, but changed the visible feed/memory question to the clean action label `Analyse Screen`.

## Round 51: Streaming Answers And Context Compaction

Made Bluey behave more like the best reference-app response surfaces while keeping the native architecture:

- Reviewed Aura's live UI pattern: one AI response element is updated as text streams in, with scroll behavior that respects the user reading older content.
- Reviewed OpenCluely's session manager pattern: keep recent conversation history and compress older/large events before model calls.
- Added an overlay `update_card` command so daemon responses can update an existing answer card instead of waiting to push a finished card.
- Added macOS and Windows overlay handling for `update_card`; macOS updates the exact visible answer card, Windows updates the current latest-card body.
- Changed the answer path to create a `BLUEY / Thinking...` card immediately, then stream provider deltas into that card as they arrive.
- Added OpenAI-compatible SSE parsing for live providers when the request is streaming.
- Added word-by-word replay for local deterministic fallback and any non-streaming path, so the user still sees progressive output.
- Added provider-context compaction: transcript, recent Q&A, documents, screenshots, and other context items are individually capped and then compacted to a total active model-window budget.
- Added tests for streaming word chunks and provider context compaction.

## Round 52: Dark Host Backdrop

Adjusted the web showcase theme behavior after product review:

- Dark/black Bluey UI now keeps the full host page backdrop dark instead of flipping the surrounding page to white.
- Updated the animated hero canvas too, so the middle/right side no longer paints a white gradient behind the dark UI mock.
- The top nav, hero copy, launch pill, section band, and feature tiles now stay in the same dark/cyan visual system when the dark UI mode is active.
- Renamed the website toggle accessibility label from page-background language to Bluey UI theme language.
- Fixed the real macOS overlay host layer as well: dark mode now applies an actual dark main-panel background, not only dark header/composer controls over a mostly transparent middle.

## Round 53: Structured Streaming Answer UI

Improved how live answers are presented in the native overlay:

- Made the bottom `Answer`, `Recap`, and `Analyse Screen` action icons render white so the controls read clearly on the dark command bar.
- Added answer-body formatting in the macOS overlay so streamed model output keeps headings, line breaks, and fenced code blocks readable while it updates.
- Styled code-block content with a monospaced face and darker code background so coding answers do not collapse into a plain paragraph.
- Kept the visible feed scrollable with retained card history, so repeated typed questions, transcript events, and streamed Bluey answers remain available in the session feed.
- Updated the provider system prompt to request overlay-friendly responses: direct answer first, short sections, and for coding questions `Approach`, `Code`, `Complexity`, and `Edge cases`.
- Added a daemon test that locks in the provider prompt shape expected by the overlay.

## Round 54: Reference-Informed Feed And Modes

Refined Bluey's input/output flow after reviewing the local Natively, OpenCluely, and Pluely reference packages:

- Natively uses dedicated streaming token channels and mode-specific intelligence events; Bluey keeps one native feed but now has stronger mode-specific answer contracts.
- OpenCluely splits assistant responses into markdown text plus separate code snippets; Bluey's macOS overlay now parses fenced code blocks and renders them as dedicated code panes.
- Pluely appends user/assistant messages, updates the active response during streaming, and auto-scrolls the live scroll area; Bluey's middle feed keeps click-through behavior and uses pointer-over wheel handling for scroll.
- Changed visible card language from mechanical `YOU / Question sent` and `BLUEY` labels to `You`, `Bluey / Response`, `System transcript`, and `Mic transcript`.
- Expanded mode defaults for General, Code, System Design, Meeting, and Writing so the selected mode changes the provider prompt shape, not just the label in the header.
- Widened the model/mode dropdowns so `System Design` and route names read cleanly.

## Round 55: Click-Through Feed Scrolling

Matched the Pinky-style caption behavior the user called out:

- Restored click-through hit testing for the middle transcript/answer feed, so normal clicks continue to reach the app underneath Bluey.
- Added local and global wheel/trackpad monitors that scroll Bluey's feed when the pointer is over the readable card area.
- Kept header controls, the bottom composer, and resize edges interactive.
- Updated README, session-flow, and icon-guide docs so future work preserves the split: click-through cards, scrollable history.

## Round 56: Readable Glass Opacity

Changed opacity semantics after the user clarified the expected behavior:

- The macOS opacity slider now controls Bluey's background glass and card surface alpha instead of fading the entire window.
- Text, icons, code blocks, and transcripts stay near full opacity even when the background is made very transparent.
- Existing cards refresh their chrome when opacity/theme changes, so old answers match the new glass setting.
- Updated README, session-flow, and icon-guide language to make this product rule explicit.

## Round 57: AI Streaming And Routing Study

Reviewed Bluey against the local OpenCluely, Pluely, Natively, Aura, and Pinky references, then applied the first production-readiness fixes:

- Added [AI-COMPETITIVE-STUDY.md](/Users/uno/Downloads/cue/docs/AI-COMPETITIVE-STUDY.md) with the reference findings, current Bluey strengths, gaps, and prioritized next work.
- Confirmed General mode still supports coding answers because the global prompt requires code-shaped output for coding questions.
- Made General mode more explicit: it now auto-detects code/debug/API/config questions and still asks for `Approach`, `Code`, `Explanation`, `Complexity`, and `Edge cases` with fenced code blocks.
- Changed overlay `Bluey Auto` routing to use the managed commercial route with fallbacks instead of a brittle direct managed-only route.
- Replaced managed pseudo-model fallback IDs with direct-provider-safe defaults for Cerebras, Groq, and OpenAI.
- Changed macOS and Windows overlay ask payloads so Auto sends `provider=auto`, letting the daemon own routing.
- Fixed answer instructions so selected mode rules merge with session/notepad rules instead of replacing them.
- Added daemon/core tests for Auto routing, direct provider alias cleanup, General coding shape, and mode/session instruction merging.

## Round 58: Supersession, Transcript Hygiene, And Tail Scroll

Read deeper implementation paths from the local reference packages and applied the next production-hardening slice:

- Natively's request generation/cancellation pattern drove a daemon-side answer generation guard in Bluey.
- Pluely's request-id streaming pattern drove stale-update protection so older provider responses cannot overwrite a newer visible answer or session Q&A.
- Aura's smart-scroll pattern drove macOS feed behavior that only auto-follows while the user is at the latest card.
- Pinky's caption overlay pattern reinforced the split between click-through reading content and interactive header/composer/resize controls.
- Bluey now marks an older active answer card as superseded when a newer ask starts.
- Bluey now ignores stale answer-card updates and does not save stale answers into recent Q&A memory.
- Recent final STT duplicates are skipped per speaker so retry/re-emission noise does not poison transcript context.
- The macOS overlay now shows a small `Latest` button when streaming continues while the user is reading older feed content.
- Local fallback answers now clearly say they are context-only fallbacks and prompt for a live provider key instead of looking like a finished model answer.

## Round 59: Pill-First Release Hardening

Closed the most visible v0.1.0 regression and hardened the terminal release path:

- Changed `bluey on` to launch pill-first instead of immediately expanding the overlay.
- Restyled the macOS pill into a compact dark Bluey command capsule with logo,
  status dot, wordmark, border, and glow.
- Made overlay UI-state validation use one shared guarded state path so attach
  and instructions flows reset to Idle on success, cancel, or error.
- Changed overlay session-token generation to return a real OS-random result
  instead of panicking on entropy failure.
- Hardened helper discovery so an installed `bluey` symlink can still find the
  canonical daemon, overlay, audio, and whisper helper binaries from the
  versioned install directory.
- Narrowed the release workflow and Makefile package path to the actual
  v0.1.0 support matrix: macOS arm64 terminal tarball, not a dashboard/Tauri
  GUI bundle or unverified Windows/Linux artifacts.
- Added `scripts/install.sh` for checksum-verified macOS arm64 installs into a
  versioned `~/.local/bluey/<version>` layout with `~/.local/bin` symlinks.
- Verified packaging and installed-path launch locally with `bluey on` and
  `bluey off` through symlinks.

## Round 60: Production Readiness Documentation Audit

Turned scattered phase notes into a current production-readiness source of truth:

- Added `docs/PRODUCTION-READINESS.md` to separate implemented macOS arm64
  local-first capability from unpaid cloud/SaaS/platform work.
- Updated README, feature map, handoff, commercial path, competitive gaps,
  pre-pricing review, implementation seams, self-review, roadmap, deployment,
  installer checklist, first-version test notes, and agent onboarding so they
  no longer under-report implemented streaming/STT/RAG/local packaging work.
- Marked the real remaining blockers clearly: clean-machine macOS validation,
  cloud auth/sync/RAG/billing/admin, Windows whisper.cpp and Windows QA,
  broader platform artifacts, production OCR/vision, observability/support
  bundle, and signed installers/auto-update.
- Left historical review/design docs intact as audit records rather than
  rewriting past implementation context.

## Round 61: Reference-Informed UI Direction And Native Polish

Started a senior-product UI pass from the actual reference apps instead of
guessing from screenshots:

- Reviewed Pinky's compact native pill, heartbeat/status dot, split
  interactive/readable zones, scroll history, and resize stability notes.
- Reviewed Natively's chat overlay for streamed markdown, copy controls, code
  block rendering, and meeting-context follow-up flow.
- Reviewed Pluely's sticky composer/history rhythm and screenshot-selection
  affordances.
- Reviewed Aura/OpenCluely's markdown streaming, code-block headers, and visible
  recording/interaction states.
- Added `docs/UX-UI-PRODUCT-DIRECTION.md` as the current UX/UI brief and
  scenario checklist.
- Polished the macOS expanded overlay from a plain debug panel into a darker
  product surface with a status strip, bordered feed, composer capsule,
  icon-led actions, role-labeled cards, accent rails, and streaming status.
- Fixed local release smoke for `./target/release/bluey on`: the macOS overlay
  build script now mirrors `bluey-overlay-macos` and `cue-overlay-macos` into
  existing Cargo target directories, so the hardened install-dir verifier can
  still require side-by-side helpers without breaking local release runs.

## Round 62: Launch Flow And Compact Pill Pass

Tightened the first-run/product flow after comparing the current overlay against
the earlier native transcript overlay and the Pinky/Natively/Pluely references:

- Reduced the macOS pill from a chunky badge to a smaller command-layer control
  (`112x28`) with tighter logo/dot/type spacing.
- Changed plain `bluey on` to open the launcher without silently creating a
  session. `bluey on --title ...` still creates immediately for scripted/smoke
  flows.
- Added visible session actions in the expanded overlay: `New` starts a fresh
  session and `Continue` restores the latest saved session when no active
  session exists.
- Added `Analyse` to the native action row and kept `Answer`, `Attach`, `Rules`,
  and `Recap` available from the composer.
- Changed empty `Answer` clicks to ask Bluey for the latest clear question or
  useful session context instead of doing nothing.
- Added a parent-process watchdog so future orphaned macOS overlay helpers exit
  if the daemon disappears unexpectedly.
