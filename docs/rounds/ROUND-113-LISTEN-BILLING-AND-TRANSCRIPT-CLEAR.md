# Round 113 - Listen Billing And Transcript Clear

Date: 2026-06-22

## Why

Listen needed a clear user-facing rule for two related behaviors:

- Stopping Listen should settle the current STT relay against actual elapsed
  transcription time, not charge a second time or leave the user guessing.
- Starting Listen again in the same recording should keep accumulated captions
  as answer context by default, while still giving the user a fast way to clear
  captions before the next answer.

## Behavior

- Starting Listen creates an STT relay session and reserves the maximum allowed
  window for that session.
- Stopping Listen or closing the relay settles the session from actual elapsed
  seconds.
- Unused reserved credit and unused trial seconds are restored by STT
  settlement.
- Starting Listen again creates a new relay session, but the current recording
  keeps transcript context unless the user clears it or starts a new recording.
- The caption-row clear control only affects future answer context. It does not
  alter STT billing history.

## Implementation

- Added `transcript_clear_requested` to the overlay protocol.
- Added daemon handling that clears the active recording transcript,
  transcript-derived summary/action/decision fields, refreshes overlay state,
  and reindexes local RAG from the remaining session context.
- Added a macOS caption-row `x` control that appears only when caption context
  exists.
- Added a Windows native `Clear` caption control with the same daemon event.

## Verification

- `cargo check --manifest-path crates/cue-daemon/Cargo.toml`
- `swift build --package-path native/macos/cue-overlay`
- `cargo test --manifest-path server/Cargo.toml db::stt_accounting --lib`
- `x86_64-w64-mingw32-gcc -D_WIN32_WINNT=0x0601 -DUNICODE -D_UNICODE -municode native/windows/cue-overlay/main.c -o /tmp/bluey-overlay.exe -luser32 -lgdi32 -ld2d1 -ldwrite -luuid -lshell32 -lcomctl32`
