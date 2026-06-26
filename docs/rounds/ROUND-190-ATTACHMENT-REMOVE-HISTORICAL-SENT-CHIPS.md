# Round 190 - Attachment Remove Historical Sent Chips

## Trigger

Owner asked what happens when one file/screen context is removed while older sent question chips still show items like `Screen context 1` and `Screen context 2`.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Intended UX

- Bottom attachment chips are pending/current session context for future answers.
- Top chips inside a sent question bubble are historical: they show what was attached to that already-sent question.
- Removing one bottom chip removes only that exact context item from future answers and the conversation file list.
- Other pending items remain.
- Already-sent chips remain visible as the receipt of what was sent with that older question.
- If a removed screenshot was already used by a sent question, Bluey keeps the prepared image copy so the historical sent chip does not become a dead record.

## Changes

- `crates/cue-daemon/src/app.rs`
  - Detects whether a removed context item was referenced by any prior conversation turn attachment ids.
  - Preserves Bluey's prepared image copy when the removed item was already sent with a question.
  - Keeps normal cleanup for unsent prepared image copies.
  - Updates the system card copy to say the item was removed from future answers while sent question chips stay in history.
  - Added a regression test for preserving a sent prepared image copy.

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Sent attachment chip tooltip now starts with `Sent with this question`.
  - Pending/current remove button tooltip now says `Remove this file from future answers`.

## Windows Parity

- The daemon-side preservation and system-card behavior is cross-platform.
- No Windows overlay code changed in this round because the current Windows chip surface is draw-only and does not expose the macOS per-chip remove/open tooltip controls. A future Windows UI parity pass should add equivalent clickable pending chips if the Windows overlay gets the same file-strip interaction model.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo fmt --check -p cue-daemon`
- `cargo test -p cue-daemon removing_sent_attachment_preserves_bluey_prepared_image_copy --lib`
- `cargo test -p cue-daemon removing_attachment_deletes_only_bluey_prepared_image_copy --lib`
- `cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib`
- `cargo build --release -p cue-daemon`
- `native/macos/cue-overlay/build.sh`

## Local Install

Installed refreshed local binaries into `~/.bluey/bin`:

- `target/release/bluey-daemon` to `~/.bluey/bin/bluey-daemon`
- `native/macos/cue-overlay/.build/bluey-overlay-macos` to `~/.bluey/bin/bluey-overlay-macos`
- `native/macos/cue-overlay/.build/cue-overlay-macos` to `~/.bluey/bin/cue-overlay-macos`
- `native/macos/cue-overlay/.build/BlueyOverlay.app` to `~/.bluey/bin/BlueyOverlay.app`

Restarted Bluey with:

- `~/.bluey/bin/bluey off`
- `~/.bluey/bin/bluey on`
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Current local status after restart:

- daemon pid `79599`
- overlay visible `true`
- overlay capture excluded `true`
- overlay position `center`
- overlay opacity `0.92`
- active meeting id `fbdb0894-1212-4fde-87cd-c42168e25009`
- active meeting title `New recording`
- transcript segments `0`
- context items `0`

## Current State

- Removing a pending/current file or screen context removes it from future answers.
- Sent question chips continue to show the historical attachments used by that old question.
- Sent screenshot image copies are preserved when the context is removed after being sent.
- The restart created a fresh active meeting, which is expected after `bluey off && bluey on`; prior sessions remain in history.

## Remaining QA

- Manual macOS check with two pending screenshots:
  - remove one pending chip and confirm the other stays visible for the next answer.
  - send both screenshots and confirm sent chips label `Screen context 1` and `Screen context 2`.
  - remove the source context after sending and confirm the sent chips remain visible.
  - open sent screenshot chips after removal and confirm the preserved copy is still usable.
- Future Windows UI parity pass if Windows needs clickable pending chips.
