# Round 211 - Code Canvas Partial Stream Guard

## Trigger

The owner pointed at a coding answer where the right-side canvas looked broken and stopped early, showing only the beginning of a code block:

```text
CODE
----
class Node:
    def __init__(
```

Expected behavior: Bluey can stream the visible answer live, but it should not open or save a code canvas from a partial/unclosed streamed code fence.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause/Fix

- The daemon inferred a fallback `Code canvas` artifact on every `UpdateCard`, including `done: false` streaming updates.
- The fenced-code extractor intentionally accepted unclosed fences at EOF, so a half-streamed code fence could be treated as complete code.
- OpenAI-compatible streaming parsed text deltas but ignored truncation-like `finish_reason` values, so a provider length stop could be treated like a normal completed answer.

Fixes:

- Fallback answer artifact inference now only runs when an overlay answer update is final.
- Explicit managed/provider artifacts still pass through when provided at finalization time.
- Unclosed fenced code blocks are no longer extracted into code canvas bodies.
- OpenAI-compatible streams now treat truncation finish reasons such as `length` and `max_tokens` as incomplete-stream errors instead of complete answers.
- Existing user-facing incomplete-stream copy is reused: Bluey says the connection dropped before the answer finished and asks the user to retry, rather than saving a broken answer as finished.

## Mac/Windows Parity Check

- This is a shared daemon/backend fix, so it applies to macOS and Windows overlays without separate native UI code changes.
- The Mac and Windows canvas surfaces will still receive final explicit artifacts and complete fenced-code artifacts, but not inferred canvases from partial live-stream text.

## Verification

```bash
cargo fmt --manifest-path crates/cue-daemon/Cargo.toml
cargo test -p cue-daemon answer_overlay_artifact --lib
cargo test -p cue-daemon provider_length_finish_reason_is_incomplete_stream --lib
cargo test -p cue-daemon llm_overlay_artifact_keeps_sql_code_canvas --lib
cargo build -p cue-cli
cargo build -p cue-daemon --bin bluey-daemon
BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh
target/debug/bluey status
```

Final visible-mode status check:

- `overlay_visible: true`
- `overlay_capture_excluded: false`
- daemon pid refreshed to the new debug run

## Current State

- Local visible/debug Bluey has been relaunched from the fresh debug CLI and daemon build.
- The running overlay is intentionally capture-visible for QA screenshots.
- Return to normal capture-excluded mode with:

```bash
target/debug/bluey off
bluey on
```

## Remaining QA/Gates

- Manually ask a code-heavy question that produces a full fenced code block and confirm the canvas opens only once complete code exists.
- Manually test an intentionally long/truncated response and confirm Bluey shows the incomplete-stream retry message instead of a broken canvas.
- Public binaries still need a release build and deploy if this should ship to download users.
