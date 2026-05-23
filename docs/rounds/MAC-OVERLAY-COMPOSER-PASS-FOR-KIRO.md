# Mac Overlay Composer Pass

Date: 2026-05-23
Branch: `feat/phase-3-round-12`

## Product Decision

The bottom composer is the command center. It should not mirror another product's vague "Full access" language. Every visible control must map to a concrete Bluey action:

- `+` attaches files/context.
- `Style` opens the session answer-style editor.
- `Listen` / `Stop` controls mic/system capture.
- `%` controls overlay opacity.
- `Auto` selects the routing lane/model policy.
- `Screen` triggers screen analysis.
- `↑` sends the typed question or asks from current session context.

The input box is for user text only. Listening is a separate button because audio capture is a session state, not input text.

## UI Changes

- Replaced the single-line AppKit text field with a multi-line `ComposerTextView`.
- Composer grows vertically up to a capped height and stays inside the fixed overlay window.
- `Enter` sends; modified Return inserts a newline.
- Removed the ambiguous `Full access` button.
- Moved model selection into the composer control row.
- Kept overlay size bounded by the existing visual smoke gate.

## Backend Shape This UI Expects

Bluey should behave like an always-present AI workspace, not a pile of independent commands. The backend should keep these contracts strong:

1. Capture layer records source-labeled events: mic, system, screen, docs, typed user input.
2. Context compiler builds a bounded prompt from recent session state, uploaded docs, screenshots, and RAG memory.
3. Router classifies each request into instant, balanced, deep, vision, or canvas-worthy work and returns route metadata.
4. Managed provider path owns secrets, pricing, fallback, and cost attribution. Local fallback stays invisible to customers.
5. LLM responses return both chat text and optional artifact fields: `artifact_type`, `artifact_body`, `confidence`, and `cost_label`.
6. Overlay renders normal answers in chat and opens canvas only when the backend explicitly returns an artifact.
7. Local SQLite is an offline/cache queue; cloud Postgres + pgvector is the canonical multi-device/session memory once logged in.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `scripts/macos-overlay-visual-smoke.sh`
- `cargo fmt --all --check`
- `git diff --check`

