# Round 212 - Code Explanation Chat Quality

## Trigger

The owner showed Bluey answering `Build me LRU cache` with a chat body that looked like raw notes instead of a helpful explanation:

```text
**How it works:**
- `head <-> [LRU ... MRU] <-> tail`
...
**Complexity:** O(1) get and put.
```

The right-side code canvas also still showed an old partial snippet in the visible conversation. Expected behavior: chat should teach the logic cleanly, and code/canvas detail should not make the spoken answer feel broken.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause/Fix

- General and Code mode instructions were still nudging every coding topic toward patch-outline sections, even when the user asked to explain logic.
- The chat prompt did not strongly separate implementation requests from explanation-only coding questions.
- macOS and Windows overlays render answer cards as plain text, so Markdown decoration like `**...**`, `###`, and inline backticks appeared literally in the chat.
- The owner's screenshot was also showing an already-generated old card; generated history does not rewrite itself after prompt/display fixes.

Fixes:

- Shared daemon prompt now tells Bluey to teach explanation-only coding questions with a flow: core idea, data structures, operation walkthrough, invariant, complexity, and edge cases.
- General mode now avoids a Patch section for algorithm/code explanation requests unless the user asks for code changes.
- Code mode now keeps Patch for implementation/change requests, but skips Patch and teaches step-by-step for explanation-only questions.
- Chat prompt now discourages Markdown emphasis in prose.
- macOS answer rendering strips Markdown headings/bold/underline decoration from prose, while preserving fenced code bodies until the chat/canvas splitter handles them.
- macOS strips inline backtick markers only from final chat prose, so fenced code such as `__init__` is not damaged before canvas routing.
- Windows answer rendering now performs the same plain-text markdown cleanup for answer cards and is code-fence aware, preserving identifiers such as `__init__`.

## Mac/Windows Parity Check

- Prompt changes are shared daemon behavior and apply to both platforms.
- macOS and Windows both strip markdown decoration from answer-card prose.
- Both native cleanup paths preserve fenced code contents instead of rewriting code identifiers.

## Verification

```bash
cargo fmt --manifest-path crates/cue-daemon/Cargo.toml
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
cargo test -p cue-daemon mode_instructions --lib
cargo test -p cue-daemon provider_messages --lib
cargo test -p cue-daemon sanitize_answer_text --lib
cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib
cargo build -p cue-cli
cargo build -p cue-daemon --bin bluey-daemon
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh
target/debug/bluey status
```

Final visible-mode status check:

- `overlay_visible: true`
- `overlay_capture_excluded: false`
- fresh visible daemon pid `80510`
- fresh overlay process includes `--bluey-dev-overlay --bluey-local-visible-overlay --bluey-overlay-capture-visible`

## Current State

- Local visible/debug Bluey is running from the rebuilt debug daemon and rebuilt macOS overlay.
- Existing old answer cards in history can still look old because they were generated before this prompt/display round.
- New explanation-only coding answers should be less patch-note-like and should not show raw Markdown decoration in chat.

## Remaining QA/Gates

- Manually ask `Build me LRU cache` and then `explain the logic` in a fresh visible session.
- Confirm the chat teaches the logic in plain language and the canvas only contains complete code.
- Public binaries still need a release build and deploy if this should ship to download users.
