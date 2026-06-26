# Round 112 - Hot Context First Rag Timeout

Date: 2026-06-22

## What Changed

- Kept active transcript, recent Q&A, current attached docs, resume/JD context, screenshots, and notes on the immediate answer path.
- Made local RAG memory retrieval opportunistic with a small timeout before answer generation.
- Added `BLUEY_ANSWER_RAG_TIMEOUT_MS` to tune memory wait time.
- Default RAG lookup wait is 120 ms.
- Setting `BLUEY_ANSWER_RAG_TIMEOUT_MS=0` skips RAG lookup for maximum first-token speed.

## Why

The Answer button should not wait on slow embedding or memory lookup. Bluey should answer from hot context immediately, then rely on existing indexed memory only when it is already fast enough to retrieve.

## Current Behavior

- Resume, JD, current docs, screenshots, and live transcript are included directly from the active session.
- Transcript and attachment indexing continues in background.
- Older transcript memory is retrieved only if it returns inside the fast timeout.
- If memory lookup is slow, Bluey skips it for that answer instead of blocking the first visible token.

## Verification

- `cargo fmt --package cue-daemon`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape`
- `cargo build -p cue-daemon --release`

## Local Install

Copied rebuilt daemon binaries into `~/.bluey/bin`:

- `bluey-daemon`
- `cue-daemon`
