# Round 096 - Screen Context And Answer Flow - 2026-06-21

## Why

The Screen button was acting like an immediate answer request. When browser text capture failed, Bluey attached a screenshot and immediately asked a generic vision question, which produced noisy cards such as "Analyse the captured screenshot context" and left the screenshot chip visible at the bottom of the overlay.

That was confusing for the intended user flow:

1. User clicks Screen to stage current visual context.
2. User keeps typing or listening.
3. User clicks Answer when ready.
4. Bluey sends the typed question, latest transcript context, documents, and staged screenshot together.

## What Changed

- Screen now stages context instead of auto-answering.
- The macOS overlay preserves the composer text when Screen is clicked.
- The Screen badge says "Screen - ready" while capture is happening instead of implying a deep answer is running.
- Browser-readable page captures now show a concise "Screen context ready" card and wait for Answer.
- Screenshot fallback now captures a screenshot, attaches it to the session, shows a concise warning/tip, and waits for Answer.
- Screenshot/image context is hidden from the bottom document chip strip so it does not sit there like an attached document.
- Answer requests that include screenshot context are automatically promoted to the vision route when the selected route would otherwise omit image upload.

## User-Facing Behavior

Clicking Screen means "remember what is visible now." It should not spend an answer request by itself.

Clicking Answer means "answer using everything currently relevant":

- typed composer text
- live-caption transcript context
- attached documents/code/text
- staged screen capture or readable page text
- saved session context and RAG snippets, when available

If Chrome page text capture is blocked because JavaScript from Apple Events is disabled, Bluey still captures a screenshot and explains the browser setting without blocking the user.

## Local Discovery Boundary

Bluey can support a safe version of "I found useful setup files on disk" later, but it must not become credential scraping.

Allowed behavior:

- user-approved folders only
- detect file presence and non-secret metadata
- redact token-like values before display or model use
- explain what the evidence suggests, such as "a Cloudflare admin token file appears to exist"
- ask before sending any discovered text to cloud models

Disallowed behavior:

- showing full API keys, session cookies, or secrets in the overlay
- silently uploading discovered secrets
- scanning the whole disk by default
- using discovered credentials automatically

The useful answer should be based on redacted facts and file types, not raw secret values.

## Files

- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

## Verification

- `cargo test -p cue-daemon overlay_context_items_show_documents_not_screen_captures`
- `cargo test -p cue-daemon provider_messages`
- `swift build -c release --package-path native/macos/cue-overlay`

## Risk

The main behavior change is that Screen no longer immediately generates an answer. This is intentional, but any user expecting Screen to answer in one click now needs to press Answer after staging the visual context.
