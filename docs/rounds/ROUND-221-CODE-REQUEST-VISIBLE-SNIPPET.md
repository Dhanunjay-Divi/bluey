# Round 221 - Code Request Visible Snippet

Date: 2026-06-27 13:27 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a live QA answer where the user asked:

- "Two numbers without third variable ... write a code for swapping two numbers without third variable?"
- follow-up: "I want the code, in Python."

Bluey answered with vague explanation text while the canvas showed only a small snippet. The chat said "Here is the Python code" but did not actually show code inline.

## Root Cause

- The prompt already preferred complete code in fenced blocks for first-time coding/build answers, but it did not strongly cover explicit follow-ups like "I want the code, in Python."
- Managed responses can separate answer text and artifact body. If answer text is prose and the code is only in the artifact, the visible chat can feel broken even though a canvas exists.
- For small standalone coding tasks, putting code only in canvas is too hidden. The overlay answer should include the runnable snippet directly in chat and still let canvas hold the workbench.

## Fix

- Strengthened the daemon output-format prompt:
  - explicit requests for code, a program, an implementation, "I want the code", or "write code in language" must include a complete fenced code block with a language tag
  - small standalone coding tasks must include the full runnable snippet directly in chat, not only prose or a canvas artifact
- Strengthened Code mode and General mode with the same explicit-code rule.
- Added a runtime fallback for managed code artifacts:
  - if the final answer has a code artifact but the visible chat body has no fenced code
  - extract the first real code section from the artifact
  - infer a simple language tag when possible
  - append a compact fenced code preview to the visible answer
  - keep system-design compaction unchanged

## Mac / Windows Parity

- This is shared daemon/backend answer-shaping behavior.
- Mac and Windows clients both receive the same visible answer body from the daemon.
- No native Windows or macOS UI fork was needed in this round.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-daemon code_artifact_adds_preview_when_chat_body_is_vague --lib`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
- `cargo test -p cue-daemon mode_instructions_specialize_default_answer_shapes --lib`
- `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib`
- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `git diff --check`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

Latest local visible status:

- daemon pid `40009`
- overlay visible `true`
- overlay capture excluded `false`
- screen capture active `false`

## Current State

- Local visible QA mode is running from the rebuilt debug daemon/overlay.
- Explicit code follow-ups should no longer produce a prose-only answer when a code artifact exists.
- Canvas still opens for code artifacts, but the chat answer now stays useful by itself.

## Remaining QA / Gates

- Re-test "I want the code, in Python" live with the running local build.
- Before release/upload, return from visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
