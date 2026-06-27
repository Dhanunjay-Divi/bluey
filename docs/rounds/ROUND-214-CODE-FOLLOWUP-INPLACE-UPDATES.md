# Round 214 - Code Follow-Up In-Place Updates

Date: 2026-06-27 01:03 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported that coding answers could still feel broken:

- the first coding canvas could look incomplete or too summary-like
- follow-up questions on the same code should update the existing code context instead of replacing or duplicating the whole code
- changed code should be visually distinguishable
- Bluey should keep privacy-safe logs for hard-to-reproduce user scenarios without storing personal transcript/code text

## Root Cause / Fix

- The daemon prompt already separated code explanations from implementation answers, but follow-up implementation prompts still needed a stronger contract to prefer a small changed block, `PATCH`, or unified diff.
- Managed code artifacts flattened patch-style sections too aggressively, which made follow-up edits look like generic code replacements.
- The macOS canvas appended code follow-ups as generic `FOLLOW-UP` text, including the question and full artifact body, so users saw stacked blobs instead of in-place updates.

Implemented:

- Strengthened backend answer-shape rules:
  - first-time coding/build answers should provide complete code for the canvas
  - code follow-ups should prefer a changed block, `PATCH`, or unified diff
  - explanation-only follow-ups should avoid new code fences and keep the current canvas stable
- Preserved patch-like code canvas headers:
  - `PATCH`
  - `DIFF`
  - `CHANGED BLOCK`
  - `CHANGED LINES`
- Updated macOS code canvas follow-up handling:
  - patch/diff/change-block follow-ups append under `PATCH N`
  - full replacement follow-ups update the main `CODE` section in the same canvas and add a compact `CHANGED LINES N` summary
  - raw follow-up question text is no longer injected into code canvas update sections
- Added macOS canvas syntax coloring for updates:
  - patch/update headers use green
  - added lines use green
  - removed lines use red
  - diff hunk headers use cyan
  - update sections get a subtle tinted background
- Kept diagnostics privacy-safe from Round 213:
  - logs retain shape/intent/count metadata
  - logs do not store raw user code, transcript, title snippets, or private answer text

## Mac / Windows Parity

- Backend prompt/artifact behavior is shared by Mac and Windows.
- macOS received the native canvas merge/highlight UX because the visible canvas pane lives in `native/macos/cue-overlay`.
- Windows native overlay does not currently have the same rich canvas pane implementation in this branch, so no Windows native file was changed this round.
- Windows still benefits from the backend follow-up shape contract and patch artifact preservation.
- Windows syntax check was still run to ensure no shared assumptions broke the current Windows overlay.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon mode_instructions --lib`
- `cargo test -p cue-daemon general_mode_keeps_code_shape_for_coding_questions --lib`
- `cargo test -p cue-daemon llm_overlay_artifact_preserves_patch_canvas_header --lib`
- `cargo test -p cue-daemon llm_overlay_artifact --lib`
- `cargo test -p cue-daemon answer_diagnostics --lib`
- `cargo test -p cue-daemon --lib` (`268 passed; 2 ignored`)
- `cargo build -p cue-cli`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `git diff --check`

Local visible run refreshed:

- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

Latest visible status after relaunch:

- daemon pid `7350`
- overlay visible `true`
- overlay capture excluded `false`
- overlay opacity `1.0`
- screen capture active `false`

## Current State

- The local debug overlay is running in visible QA mode from `/Users/uno/Downloads/cue/target/debug/bluey`.
- First coding answers should still open a normal code canvas.
- Explanation-only code follow-ups should preserve the existing canvas and answer in chat.
- Code-changing follow-ups should now update the same code canvas as a patch/change section instead of producing a messy duplicate full answer.

## Remaining QA / Gates

- Manually test a real sequence in the overlay:
  1. ask `Build me LRU cache`
  2. ask an explanation-only follow-up such as `explain the eviction logic`
  3. ask a mutation follow-up such as `change it to return None on missing key`
  4. confirm the canvas preserves the base code and shows a colored patch/update section
- If Windows later gets the same rich canvas pane, port the macOS merge/highlight behavior there instead of relying only on the shared backend.
- Do not ship capture-visible mode. Before release/deploy, restart normally and run the visible-flag release hygiene scan.
