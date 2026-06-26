# Round 118 - One-Shot Image Context Memory

## What Changed

- Sent screen captures and image attachments are treated as one-shot visual context.
- After a successful Answer, Bluey records a lightweight text summary for the used image context.
- Bluey best-effort replaces the stored full image reference with a small local thumbnail.
- Bluey-owned full captures are removed after the thumbnail is created.
- Future answers use summaries, snippets, and RAG memory unless the user presses Screen again or explicitly attaches the image again.

## Why

This keeps visual answers useful without repeatedly spending vision tokens on the same screenshot or image. It also keeps local storage smaller over time.

## Cloud Behavior

- Current cloud sync keeps session/context metadata and text previews/RAG chunks.
- The sync path does not upload raw screenshot or image bytes.
- If R2/object storage is added for files later, prompts should still send raw images only when they are explicitly pending for that answer.

## Verification

- `cargo test -p cue-daemon sent_image_context_becomes_lightweight_memory_only -- --nocapture`
- `cargo test -p cue-daemon meeting_context_ -- --nocapture`
