# Bluey UI Canvas Pass

Date: 2026-05-21

## Goal

Make the native macOS overlay feel less like a debug feed and more like a product assistant:

- Short/simple answers stay in chat.
- Code, system design, screen analysis, document analysis, and long structured output open a side canvas automatically.
- The user can collapse the canvas and reopen it from the header.
- Chat remains readable: code blocks are removed from answer bubbles once the canvas owns them.

## Implementation

Touched file:

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

Changes:

- Added `CanvasPaneView`, `CanvasKind`, and `CanvasArtifact`.
- Added a right-side canvas pane inside the expanded overlay workspace.
- Added a header canvas toggle button. It appears only after a canvas-worthy answer exists.
- Added auto-routing from overlay cards to canvas:
  - fenced code or code-like answers -> Code canvas
  - architecture/system-design keywords -> System design canvas
  - screenshot/screen-analysis keywords -> Screen analysis
  - document/context keywords -> Document notes
  - long structured answers -> Workspace
- Changed answer bubbles so fenced code is stripped from chat and shown in the canvas instead.
- Expanded the window automatically when the canvas opens so code has enough width.
- Kept the collapsed pill unchanged.

## Visual QA

Standalone overlay run in dev-only capture-visible QA mode with a synthetic coding answer.

Important:

- Capture-visible QA requires the explicit dev overlay gate.
- Do not document or ship capture-visible env flags in customer install paths.
- Customer launch paths must keep the overlay capture-excluded.

Captured:

- `/tmp/bluey-visual-qa/empty-state-window.png`
- `/tmp/bluey-visual-qa/canvas-pill-window-v2.png`
- `/tmp/bluey-visual-qa/canvas-expanded-window-v2.png`

Result:

- Pill remains compact.
- Chat shows the question on the right and the concise Bluey response on the left.
- Code opens automatically in the right canvas.
- Canvas can be collapsed from its own header and reopened from the top header button.
- Empty/new-recording state now explains the session surface without looking like a blank debug panel.

## Product Research Notes

Reference patterns checked during this pass:

- ChatGPT Desktop: fast global entry point, screenshots/files/voice as first-class inputs.
- Raycast AI: command-first interactions, reusable AI commands, clear model/routing controls.
- Granola: recording state and meeting transcript are always understandable at a glance.
- Pieces: context/memory is the product surface, not an afterthought.
- Local references: Pinky, Pluely, Natively, OpenCluely, Aura.

Bluey implication:

- Keep the overlay compact enough to feel like a command layer.
- Keep session state, balance, context, and listening state visible.
- Treat rich answers as artifacts in a side canvas, not oversized chat bubbles.
- Avoid exposing implementation-only concepts such as local fallback as a primary user mode.

## Verification

Commands run:

```bash
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
git diff --check
```

## Follow-ups

- Persist open/collapsed canvas state per session.
- Move artifact metadata generation fully into the managed router/server once managed streaming is finalized.
- Add native visual regression smoke once the macOS overlay has a stable screenshot harness.

## Follow-up Pass: Explicit Artifact Contract + Bottom Bar Polish

Date: 2026-05-21

Goal:

- Stop making the native overlay infer every canvas decision from answer text.
- Make answer artifacts first-class IPC metadata so the daemon/server can say exactly what the UI should open.
- Tighten the bottom command bar so listening, opacity, typed ask, instructions, attach, analyse screen, and send live in one row.
- Make Hide and Close mean different things.

Implementation:

- Added `CardArtifactType` and `CueCardArtifact` in `crates/cue-core/src/cards.rs`.
- Added optional `artifact` metadata to `CueCard` and `OverlayCommand::UpdateCard`.
- Added matching managed server/client response fields on `/router/complete`:
  - `artifact_type`
  - `artifact_body`
  - `cost_label`
  - `confidence`
- Added daemon-side `answer_overlay_artifact(...)` detection as the bridge contract until the managed provider threads server metadata through `LlmResponse`.
- `OverlayAnswerStream` now sends final answer updates with:
  - `artifact.artifact_type`
  - `artifact.title`
  - `artifact.body`
  - `artifact.confidence`
  - existing `cost_label`
- The macOS overlay now consumes explicit artifact metadata first and only falls back to local heuristics when metadata is absent.
- `bluey-server` now computes the same metadata on managed responses so the server can own the decision path as the managed router matures.
- Added a compact opacity slider to the one-row composer.
- Pinned the balance label to a fixed centered width so it no longer drifts against the close controls.
- Split controls:
  - Eye slash = hide/collapse back to the pill.
  - X = confirm full shutdown, with copy: `To start again, run: bluey on`.
- Changed the transcript strip text to `Live captions preview` so it is clear this is the rolling live transcript preview, not the main chat history.

Visual QA:

- `/tmp/bluey-visual-qa/artifact-contract-polish.png`

Verification:

```bash
cargo fmt --all --check
cargo test -p cue-core overlay -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact -- --nocapture
cargo clippy -p cue-core -p cue-daemon --all-targets -- -D warnings
cd server && cargo test -p bluey-server -- --nocapture
cd server && cargo clippy --all-targets -- -D warnings
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

## Follow-up Pass: Managed Metadata Passthrough + `bluey on` Setup Clarity

Date: 2026-05-21

Goal:

- Finish the direct managed response contract: server-owned artifact metadata must travel through `cue-llm`, speculative routing, dashboard events, persistence, and the response UI.
- Keep daemon/native-overlay heuristics only as fallback for direct/local/older responses.
- Make the first `bluey on` boot card tell a fresh user whether managed answers
  are ready or browser sign-in is open.

Implementation:

- Added `LlmArtifactMetadata` plus `cost_label` and `artifact` fields to `cue_llm::LlmResponse`.
- Added matching `cost_label` and `artifact` fields to `cue_llm::LlmChunk` so `/router/complete/stream` billing events can carry the same metadata.
- `BlueyManagedProvider` now maps `/router/complete` and `/router/complete/stream` metadata into `LlmResponse` / `LlmChunk`.
- `cue-router::SpeculativeChunk` now carries cost labels and artifact metadata through draft/final lanes.
- Dashboard `request_cue` now forwards artifact metadata in `cue_response_chunk` and persists it in the final `CueResponse`.
- `CueResponse` now serializes `cost_label`, `artifact_type`, `artifact_body`, and `artifact_confidence`.
- `cue_responses` SQLite rows now preserve those fields across reloads, with additive column migration for existing local databases.
- Dashboard Responses route now renders managed artifacts as a compact canvas block under saved and in-flight responses.
- `bluey on` boot lines now include:
  - session-history affordance
  - attach/analyse consent affordance
  - transcript/answer behavior
  - managed-ready status or browser sign-in setup prompt

Verification added:

- `bluey_managed` SSE parser test now asserts billing chunks preserve `cost_label` and artifact metadata.
- `AnswerLlm` test now asserts `CueResponse` preserves cost label and artifact metadata.
- `cue_response_billing_metadata_roundtrips` now covers persisted artifact fields.
- `bluey_on_boot_lines_*` tests cover linked vs unlinked startup copy.

Remaining compatibility fallback:

- Native overlay answer cards still compute a fallback canvas artifact when a direct/local provider has no explicit metadata. That path is intentional until every provider returns managed artifact fields.

Verification:

```bash
cargo fmt --all --check
cargo test -p cue-llm bluey_managed -- --nocapture
cargo test -p cue-cli bluey_on_boot_lines -- --nocapture
cargo test -p cue-daemon test_answer_preserves_cost_metadata -- --nocapture
cargo test -p cue-daemon cue_response_billing_metadata_roundtrips -- --nocapture
cargo test -p cue-router
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cd crates/cue-dashboard/ui && npm test && npm run build
cd ../../..
cd server && cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

Debug capture safety:

- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` and `BLUEY_OVERLAY_CAPTURE_VISIBLE=1` are ignored unless `BLUEY_DEV_OVERLAY` / `--bluey-dev-overlay` is also set.
- Customer install docs and scripts must not mention or set capture-visible flags.

## Follow-up Pass: First-Use Opacity Control Clarification

Date: 2026-05-21

Goal:

- Make opacity discoverable for a new user instead of exposing an unlabeled slider.
- Keep the composer compact and avoid adding a second row.

Implementation:

- Replaced the standalone slider in the macOS composer row with a compact opacity capsule:
  - `Opacity` label
  - slider
  - live percentage label
- `OverlayCommand::SetOpacity` now updates the slider label as well as window alpha.
- Opacity is clamped to 50-100% from the UI so text remains readable while still allowing the overlay to sit back visually.
