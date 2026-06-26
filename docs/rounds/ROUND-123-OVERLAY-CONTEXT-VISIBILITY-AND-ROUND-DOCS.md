# Round 123 - Overlay Context Visibility And Round Docs - 2026-06-22

## Why

The user expected Pinky-style engineering round docs for every coding,
architecture, deployment, or product/UX pass. In this live round, "round docs"
was initially misread as rounded document chips in the overlay. This document
records the actual engineering round and re-establishes the required habit.

## Rule Going Forward

For every meaningful Bluey round:

1. Add or update a doc in `docs/rounds/` before the final response.
2. Add a `docs/WORKLOG.md` entry when the work changes shipped behavior,
   architecture, deployment state, or product direction.
3. Capture what changed, why it changed, verification run, rollout state, and
   remaining risks.
4. Keep the doc concrete enough that a future agent can continue without
   reconstructing intent from chat.

## Product Scope

- Attached documents, images, and screen captures should be visible as context,
  not hidden in a way that makes users wonder whether Answer will include them.
- The session history should show whether a conversation has saved context.
- macOS and Windows overlays should keep the same mental model for attached
  context, even when the visual polish differs.
- Local visible mode is only for live testing; normal release/deploy flows must
  keep capture-exclusion behavior.

## Code Changes In This Round

- `crates/cue-core/src/overlay.rs`
  - Added context/image counts to session list payloads.
  - Updated serialization coverage for the new optional fields.
- `crates/cue-daemon/src/app.rs`
  - Session rows now include saved context counts for history/sidebar display.
  - Overlay context items now include attached images and diagrams, not just
    documents and screen captures.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Session/history rows can render compact rounded context count badges.
  - Active context chips can include image and screen context so the user sees
    what will travel with the next answer.
- `native/windows/cue-overlay/main.c`
  - Added protocol parsing for `set_context_items`.
  - Added read-only rounded context chips in the Windows overlay so document and
    image context is visible there too.
- `native/windows/cue-overlay/build.ps1`
  - Keeps shell drag/drop support linked into the Windows overlay build.

## Architecture Notes

- Context visibility is UI/protocol metadata only. The provider payload is still
  built by the daemon from the saved `MeetingRecord` context.
- Image payload filtering and caps remain the server/daemon responsibility; the
  overlay should show what is attached, not decide provider eligibility.
- The round-doc rule is a process invariant, not a UI feature. It should be
  followed for backend, billing, infra, macOS, Windows, web, and prompt/system
  design rounds.

## Verification

- `cargo test -p cue-core set_sessions_serializes_as_overlay_command`
- `cargo test -p cue-daemon overlay_context_items_show_documents_images_and_screen_captures`
- `swift build -c release --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-g++ -D_WIN32_WINNT=0x0601 -DUNICODE -D_UNICODE -municode native/windows/cue-overlay/main.c -o /tmp/bluey-overlay.exe -luser32 -lgdi32 -ld2d1 -ldwrite -luuid -lshell32`
- `cargo build --release -p cue-cli -p cue-daemon`
- Local install refreshed under `~/.bluey/bin` and visible mode restarted for
  live testing.

## Remaining Follow-Ups

- Confirm in the running macOS overlay that attached PNG/HEIC/JPEG files show in
  the active context strip and send with Answer.
- If old sessions still look empty, check whether those sessions actually have
  saved `context` artifacts; the UI cannot show context that was never saved.
- Keep future final responses tied to their round doc when any meaningful code
  or architecture changed.
