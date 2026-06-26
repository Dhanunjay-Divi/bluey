# Round 196 - Screen Context Payload Guard

## Trigger

Owner showed an overlay answer card that failed with:

`Bluey could not complete that answer yet. Try again, or check the server logs for the detailed provider error.`

The question had two attached screen-context chips. Local logs for the matching request showed the managed vision route failing with HTTP `413 Payload Too Large` and response text `Failed to buffer the request body: length limit exceeded`.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 15:17 EDT

## Root Cause

- The desktop attached multiple screen captures as provider image data URLs and routed the answer to managed vision.
- The daemon had a per-image cap, but no total per-answer image payload cap.
- The server image validator allowed screen images, but `/router/complete` and `/router/complete/stream` did not have an explicit body limit aligned with that support.
- Axum rejected the request before Bluey validation ran, returning `413 Payload Too Large`.
- The daemon only saw `provider error: server error: 413` and collapsed it into the generic "could not complete" message.

## Implemented

- Local daemon:
  - Added a total per-answer screen image upload budget of 12 MB.
  - If additional screenshots would exceed the budget, Bluey omits those image bytes from provider upload and keeps the saved text preview in the prompt.
  - Added user-facing classification for `413`, `payload too large`, `length limit exceeded`, and related screen-image validation phrases.
  - The overlay now gets a clear message telling the user the attached screen context was too large and to remove one screenshot or retry smaller.
- Managed server:
  - Added an explicit 20 MB body limit to `/router/complete` and `/router/complete/stream`.
  - Tightened per-image validation to 4 MB and total image payload validation to 12 MB so server and desktop budgets match.
  - Added validation reasons for oversized total image payloads.

## Files Touched

- `crates/cue-daemon/src/app.rs`
- `server/src/api/mod.rs`
- `server/src/api/router.rs`
- `docs/rounds/ROUND-196-SCREEN-CONTEXT-PAYLOAD-GUARD.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `cargo test -p cue-daemon upload_budget -- --nocapture`
- `cargo test -p cue-daemon oversized_screen_context -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml complete_image_validation_rejects -- --nocapture`

Notes:

- The daemon was not running when inspected during this round, so the screenshot failure was diagnosed from the saved Bluey log, not by replaying a live overlay request.
- `cargo fmt --manifest-path server/Cargo.toml` was run during the round, but unrelated pre-existing rustfmt churn was removed from the final diff.

## Current State

- The exact screenshot failure was a request-size failure, not a model-answering or canvas-routing bug.
- Two or more large screen captures should no longer fail with the generic provider-error card.
- If the request is still too large, Bluey should now explain that the attached screen context is too large instead of telling the user to inspect server logs.
- Bluey can still use OCR/text previews for screen contexts that are omitted from provider image upload.

## Remaining QA Gates

- Deploy the server change before expecting production `https://bluey.sh` to accept larger managed vision requests.
- Run a live managed-vision smoke after deploy:
  - attach two normal screen captures
  - ask an answer
  - confirm it streams instead of `413`
  - attach several oversized captures
  - confirm Bluey either answers from previews or shows the new clear payload message.
- Consider adding a small UI hint on screen chips when a capture is preview-only because image upload budget was reached.
