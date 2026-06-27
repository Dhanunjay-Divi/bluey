# Round 222 - History Code Artifact Restore

Date: 2026-06-27 13:50 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a live QA case where a code answer initially had the code/canvas action, but after switching to another chat and returning, the `{}` code button disappeared. The answer body also fell back to prose-only text in the restored chat.

## Root Cause

- Live answer cards can carry `CueCardArtifact` metadata while the answer is streaming.
- Saved `ConversationTurn` records only stored question, answer, attachments, source, provider, and timestamp.
- When history/reopened-session cards were rebuilt, Bluey only had the saved prose answer, so the restored card lost its code artifact and rendered only copy/details actions.
- The code detector was also too strict for tiny valid Python snippets such as `a, b = b, a`, which made simple interview answers easier to lose during fallback inference.

## Fix

- Added optional `artifact: CueCardArtifact` to `ConversationTurn`, with serde defaults so old saved sessions still load.
- When an answer finishes, the daemon now persists:
  - the same visible answer body the overlay should show
  - the inferred or managed code/system-design artifact
- History replay now restores `turn.artifact` onto answer cards.
- Old saved turns without artifact metadata can still infer a code artifact from fenced code in the saved answer.
- Cloud sync fallback now preserves conversation artifact fields when turns are uploaded as cue responses and restores them during cloud hydration.
- Relaxed code detection for explicit code/artifact content so small assignment snippets still count as real code.

## Mac / Windows Parity

- This is shared core/daemon/history/sync behavior.
- Mac and Windows overlays both receive restored answer cards with the same artifact metadata.
- Native overlay syntax checks passed for both platforms; no platform-specific UI fork was required.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-daemon overlay_history_cards_restore_code_artifact_button --lib`
- `cargo test -p cue-daemon overlay_history_cards_infer_code_artifact_for_old_saved_turns --lib`
- `cargo test -p cue-daemon conversation_sync_preserves_code_artifact_fields --lib`
- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
- `cargo test -p cue-core --lib`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `cargo test -p cue-daemon --lib`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `git diff --check`

## Current State

- New code answers should keep their `{}` code/canvas button after switching chats and returning.
- Simple fenced snippets like Python tuple swap now remain eligible for code artifacts.
- Older saved sessions with fenced code can regain a code canvas action during replay even if they predate the new `ConversationTurn.artifact` field.
- Local visible QA mode was restarted from the rebuilt debug binary:
  - daemon pid `60912`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`

## Remaining QA / Gates

- Live-test: ask for code, switch to another chat, return, and confirm the `{}` action remains.
- Before release/upload, return from visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
