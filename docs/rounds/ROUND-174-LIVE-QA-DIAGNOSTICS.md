# Round 174 - Live QA diagnostics, 2026-06-25

## What was tested

- Restarted Bluey in local visible overlay mode.
- Queried daemon status, audio status, AI status, and cloud status.
- Injected no-credit overlay cards through the real daemon IPC to test canvas routing.
- Captured daemon lifecycle logs and a desktop screenshot after the final run.

## Findings

- Audio devices are visible, but this local install reports `native capture: no` and `runtime ready: no`.
- Cloud account sync is ready, but `bluey ai status` still reports managed provider credentials missing for the managed lane.
- RAG embedding failures existed without enough source detail, making document/image indexing problems hard to trace.
- Canvas routing preserved a coding canvas for an unrelated plain answer such as `What is a VPC?`.

## Changes

- RAG indexing warnings now include session id, source kind, source id, title, path, chunk count, and text size.
- Missing converted Markdown warnings now include the artifact and session metadata.
- Overlay event failures now include the overlay event kind.
- Canvas lifecycle diagnostics now log when the canvas opens, closes, preserves, appends, replaces, or ignores an artifact.
- Canvas preservation now requires relevance to the active canvas instead of treating every `what is` or `explain` answer as related.

## Verified

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo fmt --check -p cue-daemon`
- `native/macos/cue-overlay/build.sh`
- `cargo test -p cue-daemon managed_embedder -- --nocapture`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
- `cargo build --release -p cue-cli -p cue-daemon`
- Local visible Bluey restart through `scripts/bluey-visible-local.sh`
- Final no-credit IPC test:
  - coding answer opened a code canvas
  - related two-pointer follow-up preserved it
  - unrelated VPC answer closed it
  - later plain answer did not reopen stale canvas

## Still open

- Native mic/system capture is not linked in the local installed build, so live Listen cannot produce real transcripts yet.
- Managed AI status and cloud token status disagree, so the managed route credential path needs a focused follow-up.
