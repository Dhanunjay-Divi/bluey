# Round 110 - Cost-Safe Context And Batch RAG

## What Changed

- Saved session documents and old screenshots stay available as session context, but they are no longer copied into every provider prompt by default.
- Newly pending documents and screenshots are still sent with the specific Answer that used them.
- Answer-time RAG now embeds the question once and reuses that vector for current-session and global memory search.
- Managed document indexing now sends document chunks through `/router/embed/batch`, so a document is billed as one aggregate embedding request instead of many tiny per-chunk requests.

## Why

The UI already made attachments feel one-shot, but the provider prompt could still include saved artifacts later. That increased input tokens and could move ordinary follow-ups onto expensive vision or large-context paths. The new behavior keeps hot context fast while letting summaries and retrieved snippets carry older documents.

## Verified

- `cargo fmt --check`
- `cargo test -p cue-daemon meeting_context_ -- --nocapture`
- `cargo test -p cue-daemon rag -- --nocapture`
- `cargo test -p cue-cloud-client -- --nocapture`
- `cargo test -p cue-router -- --nocapture`
- `git diff --check`
