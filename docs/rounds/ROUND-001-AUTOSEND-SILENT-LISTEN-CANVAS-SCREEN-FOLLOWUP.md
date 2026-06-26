# Round 001 - Auto-Send, Silent Listen, Canvas, And Screen Follow-Up

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25

## Goal

Address the live tester report that Bluey could send answers when Listen had no useful transcript, repeat the same attached-context question, show duplicate voice captions, leave an old coding canvas open for a different next question, render recommendation lists as one clubbed paragraph, and make multiple attached screen captures look like only one image was sent.

## Changes

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Migrated the default auto-send mode to off for unmigrated installs, instead of defaulting to system-audio auto-send.
  - Blocked manual Answer while a Listen run is active or freshly stopped with no typed question and no captured transcript, so Bluey does not fall back to a generic attached-context prompt.
  - Added an 8 second duplicate-submit guard for identical question plus attachment-id payloads.
  - Cleared stale auto-send buffers on failed/ready/reset states.
  - Compacted near-identical Mic/System captions and prefers the Mic copy when both sources captured the same words.
  - Added an overlay-side formatting pass that splits inline bullets and `Rationale:` style headings into readable lines even if streamed chunks arrive awkwardly.

- `crates/cue-daemon/src/app.rs`
  - Numbered multiple screen attachments in the visible question/card attachment labels, such as `Screen context 1`, `Screen context 2`, so multi-screen sends are obvious.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo fmt --check -p cue-daemon`
- `cargo test -p cue-daemon sanitize_answer_text_splits_inline_recommendation_lists --lib`
- `cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib`
- `cargo build --release -p cue-cli -p cue-daemon`
- `native/macos/cue-overlay/build.sh`

## Local Install

- Installed rebuilt `bluey`, `bluey-daemon`, `bluey-overlay-macos`, `cue-overlay-macos`, and `BlueyOverlay.app` into `~/.bluey/bin`.
- Restarted local visible mode with `./scripts/bluey-visible-local.sh`.

## Current App State

- `bluey status` after restart:
  - pid `83045`
  - meeting id `a58449b9-d813-414f-a181-ca1b0b45f153`
  - title `New recording`
  - transcript segments `0`
  - context items `0`
  - overlay visible `true`
  - overlay capture excluded `false` because this is visible local test mode
  - overlay position `center`
  - overlay opacity `0.92`
  - screen capture active `false`

## Manual QA Still Useful

- In the visible overlay, start Listen, stay silent, press Answer, and confirm Bluey shows `No captions yet` instead of sending a generic card.
- Attach two or three screen captures and press Answer; the question card should show multiple numbered `Screen context` attachments.
- Ask a coding question that opens a canvas, then ask an unrelated non-coding question; the old canvas should close or stay collapsed instead of remaining as the active workbench.
