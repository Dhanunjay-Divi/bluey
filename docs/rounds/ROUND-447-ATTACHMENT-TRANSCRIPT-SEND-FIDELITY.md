# ROUND-447 Attachment And Transcript Send Fidelity

Date: 2026-07-08
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Issues

- Attaching expected readable files like DOCX, PPT, PDF, and similar documents should work consistently.
- Video files such as MP4/MOV should not appear supported unless Bluey can actually read or transcribe them.
- Live transcription should keep accumulating until the user sends it.
- When the user sends a live transcript, the sent question card should show the actual transcript text, not a generic hidden prompt.

## Changes

- Added PowerPoint support to native file filters:
  - macOS overlay drag/drop.
  - macOS `BlueyFilePicker.app`.
  - daemon AppleScript fallback picker.
  - Windows overlay drag/drop.
  - Windows file picker filter.
- Added PowerPoint recognition to daemon document classification.
- Kept MP4/MOV/video files intentionally unsupported for file attach, with clearer user-facing copy saying video files are not readable context yet.
- Changed macOS live transcript send behavior so long transcript sends preserve the actual transcript text in the sent question instead of falling back to `Answer the latest live captions...`.

## Intentional Boundary

This round does not claim MP4/MOV ingestion. To support that properly, Bluey needs a media-file transcription lane that extracts audio, runs STT, persists transcript/audio audit artifacts, and bills it separately from normal document context.

## Verification

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml doc_conversion --quiet`
- `git diff --check`

## Follow-Up Checks

- Test attaching `.pptx`, `.docx`, `.pdf`, `.xlsx`, `.md`, `.png`, and `.mov` in a local overlay build.
- Confirm sent transcript cards show the transcript text for both short and long live captions.
- If users need MP4/MOV, implement an explicit media transcription feature rather than treating videos as document attachments.
