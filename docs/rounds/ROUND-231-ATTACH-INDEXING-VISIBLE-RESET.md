# Round 231 - Attach Indexing Visible Reset

Date: 2026-06-29 11:42 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner attached a document, saw `Indexing...`, and then could not see the attached document.

## Findings

- The active local meeting had `context_items: 0` and `active-meeting.json` had `context: []`, so the current session did not actually retain an attached document.
- `handle_attach_paths` returned early when the picker returned no paths. That left the macOS overlay in the optimistic `Docs loading / Indexing selected files...` placeholder because no fresh `set_context_items` message was sent.
- The same no-refresh path could happen when all selected files were skipped or failed conversion.
- The macOS attachment strip only showed pending attachments by default. If a file existed in the conversation but was not marked pending for the next answer, the badge could say `Show 1 file` while the strip looked empty.

## Fix

- Added a daemon helper that always refreshes the overlay's current context list.
- Empty/canceled attach attempts now send `set_context_items` with the current context list, so the overlay exits `Indexing...` deterministically.
- All-skipped/all-failed attach attempts also refresh the overlay context list and sessions before returning.
- macOS now defaults to showing saved attached files when there are no pending attachment chips, so an attached document remains visible instead of disappearing behind the badge.
- After sending pending attachments, macOS now shows saved conversation files instead of clearing the strip completely.

## Mac / Windows Parity

- The daemon refresh fix is shared across macOS and Windows attach flows.
- macOS received the attachment-strip visibility fix because this pending-vs-saved chip UI lives in the Swift overlay.
- Windows already uses its own context-chip drawing path and does not have the same `Show files` pending/saved strip behavior.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon overlay_context_items --lib`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `native/macos/cue-overlay/build.sh`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA daemon restarted with the patched debug daemon/overlay.
- daemon pid `16450`
- active meeting id `93e731e4-a161-4f83-8f98-6019c86bfb92`
- overlay visible `true`
- overlay capture excluded `false` because this is visible QA mode
- transcript segments `0`
- context items `0`

## Remaining QA / Gates

- Attach a known small text/PDF/DOCX file through the paperclip and confirm:
  - `Indexing...` changes to the file badge/strip quickly
  - `bluey status` increments `context_items`
  - the chip remains visible after it becomes saved conversation context
- Try canceling the picker and confirm the overlay returns to `Docs empty` instead of staying on `Indexing...`.
- Before release/upload, return to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
