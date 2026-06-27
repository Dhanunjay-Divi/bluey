# Round 213 - Privacy Safe Answer Diagnostics

## Trigger

The owner pointed out that future reports may be hard to reproduce because the triggering data is user-private: transcripts, screenshots, files, and interview/context content cannot be copied into debugging threads. Bluey therefore needs useful log lines that explain routing, answer shape, canvas decisions, and confidence without logging personal data.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause/Fix

- Existing diagnostics were too weak for answer-quality issues, because they did not consistently capture why Bluey chose a route, opened a canvas, or treated an answer as code/design.
- Some existing overlay lifecycle logs included compact question/title snippets, which is not acceptable for user-private sessions.
- The daemon also logged raw overlay events with debug formatting, which could include private question text.

Fixes:

- Added daemon-side redacted answer request diagnostics:
  - request id
  - source label
  - primary route/provider and fallback count
  - streaming flag
  - visible context count and pending context ids count
  - question char/word counts
  - coarse question intent such as `code_explanation`, `code_or_debug`, `system_design`, `short_query`, or `general`
  - context counts by kind: screenshots, documents, transcripts, memory, other
- Added daemon-side redacted answer completion diagnostics:
  - provider
  - latency
  - token counts
  - source count
  - answer char/line/bullet counts
  - closed code block count
  - unclosed code fence flag
  - markdown emphasis / inline code marker flags
  - inferred artifact type
  - inferred artifact confidence percent
  - inferred artifact body char count
- Added final overlay answer diagnostics for card/generation id, answer shape, artifact type, artifact confidence percent, and artifact body size.
- Added warning logs for provider stream truncation and managed stream completion failures without answer text.
- Replaced raw overlay event logging with event-kind-only logging.
- Replaced raw overlay error/stdout logging with length-only logging.
- Replaced daemon lifecycle `overlay_detail` logging with `overlay_detail_chars`.
- Replaced macOS canvas lifecycle question/title snippets with redacted fields:
  - question chars/words
  - question intent
  - body chars/lines
  - artifact kind
  - artifact title/body char counts
- Existing old local logs generated before this round are not retroactively scrubbed.

## Mac/Windows Parity Check

- The main diagnostics are in the shared daemon, so both macOS and Windows get the same answer/canvas/provider logging.
- macOS had additional native canvas lifecycle details, so those were scrubbed in this round.
- Windows did not have equivalent native canvas lifecycle snippet logs in this path.

## Verification

```bash
cargo fmt --manifest-path crates/cue-daemon/Cargo.toml
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
cargo test -p cue-daemon answer_diagnostics --lib
cargo test -p cue-daemon answer_context_diagnostics --lib
cargo test -p cue-daemon answer_overlay_artifact --lib
cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib
cargo test -p cue-daemon mode_instructions --lib
cargo build -p cue-cli
cargo build -p cue-daemon --bin bluey-daemon
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh
target/debug/bluey status
```

Final visible-mode status check:

- `overlay_visible: true`
- `overlay_capture_excluded: false`
- fresh visible daemon pid `88714`

## Current State

- Local visible/debug Bluey is running from the rebuilt debug daemon and rebuilt macOS overlay.
- Future answer-quality issues should be diagnosable from shape/route/canvas/confidence logs without needing user-private content.
- Old pre-fix log files may still contain snippets from prior rounds and are not changed by this code patch.

## Remaining QA/Gates

- Generate a fresh code explanation and inspect the newly emitted log lines for:
  - `answer request diagnostics`
  - `answer completion diagnostics`
  - `overlay final answer diagnostics`
  - no raw question, answer, file, transcript, or screen text
- Consider adding a support command later that exports a redacted diagnostic bundle and optionally excludes old pre-Round-213 logs.
